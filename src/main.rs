extern crate google_drive3 as drive3;

use std::{
    convert::{TryFrom, TryInto},
    io::Write,
};

use drive3::{oauth2, DriveHub};
use hyper::body::to_bytes;

struct File {
    id: String,
    name: String,
    mime_type: String,
    owned_by_me: bool,
}

#[derive(Debug)]
enum ConversionError {
    BadId,
    BadName,
    BadMimeType,
    BadOwnedByMe,
}

impl TryFrom<&drive3::api::File> for File {
    type Error = ConversionError;

    fn try_from(value: &drive3::api::File) -> Result<Self, Self::Error> {
        Ok(File {
            id: value.id.clone().ok_or(ConversionError::BadId)?,
            name: value.name.clone().ok_or(ConversionError::BadName)?,
            mime_type: value
                .mime_type
                .clone()
                .ok_or(ConversionError::BadMimeType)?,
            owned_by_me: value.owned_by_me.ok_or(ConversionError::BadOwnedByMe)?,
        })
    }
}

#[tokio::main]
async fn main() {
    let file = std::fs::File::open("./client_id.json").unwrap();
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
    let files: Vec<File> = r
        .files
        .unwrap()
        .iter()
        .map(|f| f.try_into().unwrap())
        .collect();
    for f in &files {
        println!("{} '{}' {} {}", f.id, f.name, f.mime_type, f.owned_by_me);
    }

    'file_loop: for f in files {
        if f.mime_type == "application/vnd.google-apps.document" && f.owned_by_me {
            let response = hub
                .files()
                .export(&f.id, "application/vnd.oasis.opendocument.text")
                .doit()
                .await;
            if let Err(e) = &response {
                if let drive3::Error::BadRequest(x) = e {
                    for e in &x.error.errors {
                        if e.domain == "global" && e.reason == "exportSizeLimitExceeded" {
                            eprintln!(
                                "WARNING: Unable to download '{}' as .odt as it is too large!",
                                f.name
                            );
                            continue 'file_loop;
                        }
                    }
                } else {
                    eprintln!("Unexpected error when downloading '{}' - aborting", f.name);
                    eprintln!("error is\n{:#?}", e);
                }
            }

            let v = response.unwrap();
            let (_parts, body) = v.into_parts();
            let content = to_bytes(body).await.unwrap();
            let replacer = regex::Regex::new("[^[:alnum:]-_]").unwrap();
            let out_name = replacer.replace_all(&f.name, "_");
            println!(
                "downloaded '{}' as '{}' ({} bytes)",
                f.name,
                out_name,
                content.len()
            );
            let mut f = std::fs::File::create(format!("downloads/{}.odt", out_name)).unwrap();
            f.write_all(&content).unwrap();
        }
    }
}
