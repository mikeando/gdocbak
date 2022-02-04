extern crate google_drive3 as drive3;

use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    io::{BufReader, Write},
};

use drive3::{oauth2, DriveHub};
use hyper::body::to_bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
enum State {
    Downloaded,
    TooLarge,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileMapEntry {
    name: String,
    modified_time: String,
    state: State,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileMap {
    entries: BTreeMap<String, FileMapEntry>,
}

impl FileMap {
    pub(crate) fn needs_download(&self, f: &File) -> bool {
        match self.get(&f.id) {
            Some(entry) => entry.modified_time != f.modified_time,
            None => true,
        }
    }

    pub(crate) fn update(&mut self, f: &File) {
        if let Some(e) = self.entries.get_mut(&f.id) {
            e.modified_time = f.modified_time.clone();
            e.name = f.name.clone();
            return;
        }
        self.entries.insert(
            f.id.clone(),
            FileMapEntry {
                modified_time: f.modified_time.clone(),
                name: f.name.clone(),
                state: State::Downloaded,
            },
        );
    }

    fn get(&self, id: &str) -> Option<&FileMapEntry> {
        self.entries.get(id)
    }

    pub(crate) fn mark_as_large(&mut self, f: File) {
        if let Some(e) = self.entries.get_mut(&f.id) {
            e.modified_time = f.modified_time.clone();
            e.name = f.name.clone();
            e.state = State::TooLarge;
            return;
        }
        self.entries.insert(
            f.id.clone(),
            FileMapEntry {
                modified_time: f.modified_time.clone(),
                name: f.name.clone(),
                state: State::TooLarge,
            },
        );
    }
}

#[derive(Debug)]
struct File {
    id: String,
    name: String,
    mime_type: String,
    owned_by_me: bool,
    modified_time: String,
    trashed: bool,
}

#[derive(Debug)]
enum ConversionError {
    MissingFieldId,
    MissingFieldName,
    MissingFieldMimeType,
    MissingFieldOwnedByMe,
    MissingFieldModifiedTime,
    MissingFieldTrashed,
}

impl TryFrom<&drive3::api::File> for File {
    type Error = ConversionError;

    fn try_from(value: &drive3::api::File) -> Result<Self, Self::Error> {
        Ok(File {
            id: value.id.clone().ok_or(ConversionError::MissingFieldId)?,
            name: value
                .name
                .clone()
                .ok_or(ConversionError::MissingFieldName)?,
            mime_type: value
                .mime_type
                .clone()
                .ok_or(ConversionError::MissingFieldMimeType)?,
            owned_by_me: value
                .owned_by_me
                .ok_or(ConversionError::MissingFieldOwnedByMe)?,
            modified_time: value
                .modified_time
                .clone()
                .ok_or(ConversionError::MissingFieldModifiedTime)?,
            trashed: value.trashed.ok_or(ConversionError::MissingFieldTrashed)?,
        })
    }
}

#[tokio::main]
async fn main() {
    // Load information about files that have already been downloaded.
    // Open the file in read-only mode with buffer.
    let mut filemap: FileMap = match std::fs::File::open("downloads/filemap.json") {
        Ok(file) => {
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).unwrap()
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => FileMap {
            entries: BTreeMap::new(),
        },
        Err(e) => {
            panic!("Unexpected error reading filemap.json: {}", e);
        }
    };

    let file = std::fs::File::open("./client_id.json").unwrap();
    let console_app_secret: oauth2::ConsoleApplicationSecret =
        serde_json::from_reader(file).unwrap();
    let secret = console_app_secret.installed.unwrap();

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
        .q("mimeType='application/vnd.google-apps.document'")
        .param(
            "fields",
            "files(id,name,mimeType,ownedByMe,md5Checksum,modifiedTime,size,trashed,version)",
        )
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

    'file_loop: for f in files {
        if f.mime_type == "application/vnd.google-apps.document" && f.owned_by_me && !f.trashed {
            if !filemap.needs_download(&f) {
                println!("skipping {} as it is up-to-date", f.name);
                continue;
            }

            let response = hub
                .files()
                .export(&f.id, "application/vnd.oasis.opendocument.text")
                .doit()
                .await;
            if let Err(drive3::Error::BadRequest(x)) = &response {
                for e in &x.error.errors {
                    if e.domain == "global" && e.reason == "exportSizeLimitExceeded" {
                        eprintln!(
                            "WARNING: Unable to download '{}' as .odt as it is too large!",
                            f.name
                        );
                        filemap.mark_as_large(f);
                        continue 'file_loop;
                    }
                }
            }
            if let Err(e) = &response {
                eprintln!("Unexpected error when downloading '{}' - aborting", f.name);
                eprintln!("error is\n{:#?}", e);
                panic!();
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
            let mut ff = std::fs::File::create(format!("downloads/{}.odt", out_name)).unwrap();
            ff.write_all(&content).unwrap();
            filemap.update(&f);
        }
    }

    let mut filemap_file = std::fs::File::create("downloads/filemap.json").unwrap();
    serde_json::to_writer_pretty(&mut filemap_file, &filemap).unwrap();
}
