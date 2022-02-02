extern crate google_drive3 as drive3;

use std::io::Write;

use drive3::{oauth2, DriveHub};
use hyper::body::to_bytes;

#[tokio::main]
async fn main() {
    let file = std::fs::File::open("./python_version/drmikeando/client_id.json").unwrap();
    let console_app_secret: oauth2::ConsoleApplicationSecret =
        serde_json::from_reader(file).unwrap();
    let secret = console_app_secret.installed.unwrap();

    // Instantiate the authenticator. It will choose a suitable authentication flow for you,
    // unless you replace  `None` with the desired Flow.
    // Provide your own `AuthenticatorDelegate` to adjust the way it operates and get feedback about
    // what's going on. You probably want to bring in your own `TokenStorage` to persist tokens and
    // retrieve them from storage.
    let auth = oauth2::InstalledFlowAuthenticator::builder(
        secret,
        oauth2::InstalledFlowReturnMethod::Interactive,
    )
    .persist_tokens_to_disk("tokencache.json")
    .build()
    .await
    .unwrap();

    let hub = DriveHub::new(
        hyper::Client::builder().build(hyper_rustls::HttpsConnector::with_native_roots()),
        auth,
    );
    let r = hub
        .files()
        .list()
        .param("fields", "files(id,name,mimeType,ownedByMe)")
        .doit()
        .await
        .unwrap()
        .1;
    if r.next_page_token.is_some() {
        panic!("PAGINATION NOT YET IMPLEMENTED!")
    }
    for f in r.files.as_ref().unwrap() {
        println!(
            "{} '{}' {} {}",
            f.id.as_ref().unwrap(),
            f.name.as_ref().unwrap(),
            f.mime_type.as_ref().unwrap(),
            f.owned_by_me.unwrap()
        );
    }

    for f in r.files.as_ref().unwrap() {
        if f.mime_type.as_ref().unwrap() == "application/vnd.google-apps.document"
            && f.owned_by_me.unwrap()
        {
            let v = hub
                .files()
                .export(
                    f.id.as_ref().unwrap(),
                    "application/vnd.oasis.opendocument.text",
                )
                .doit()
                .await
                .unwrap();
            let (_parts, body) = v.into_parts();
            let content = to_bytes(body).await.unwrap();
            let replacer = regex::Regex::new("[^[:alnum:]-_]").unwrap();
            let out_name = replacer.replace_all(f.name.as_ref().unwrap(), "_");
            println!(
                "downloaded '{}' as '{}' ({} bytes)",
                f.name.as_ref().unwrap(),
                out_name,
                content.len()
            );
            let mut f = std::fs::File::create(format!("downloads/{}.odt", out_name)).unwrap();
            f.write_all(&content).unwrap();
        }
    }
}
