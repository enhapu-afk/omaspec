//! The desktop's own dialog to save a file, as other programs open it: on
//! Linux the FileChooser of xdg-desktop-portal, which Omarchy's GTK portal
//! answers. It asks where, under which name, and the choices given
//! (an export's colors), and confirms before it replaces a file.
//!
//! Where no portal answers (another desktop, a system without one), the
//! answer is [`Answer::Unavailable`] and the editor asks in its bottom line.

use std::path::PathBuf;

/// What the dialog asks.
#[derive(Debug, Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct Asked {
    pub title: String,
    /// The button that saves (`Export`).
    pub accept: String,
    /// The folder it opens in, and the name it suggests.
    pub folder: PathBuf,
    pub name: String,
    /// The kind of file: its label, MIME type and pattern (`*.svg`).
    pub kind: (String, String, String),
    pub choices: Vec<Choice>,
}

/// A choice beside the file's name: an id, a label, the options as keys
/// and labels, and the key chosen at first.
#[derive(Debug, Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, String)>,
    pub initial: String,
}

/// What the dialog answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The file to write, and the key of each choice made.
    Chosen {
        path: PathBuf,
        choices: Vec<(String, String)>,
    },
    Cancelled,
    /// No dialog could be shown, and why.
    Unavailable(String),
}

impl Answer {
    /// The key chosen for the choice `id`.
    pub fn choice(&self, id: &str) -> Option<&str> {
        match self {
            Answer::Chosen { choices, .. } => choices
                .iter()
                .find(|(chosen, _)| chosen == id)
                .map(|(_, key)| key.as_str()),
            _ => None,
        }
    }
}

/// Opens the dialog and waits for its answer.
#[cfg(target_os = "linux")]
pub async fn save(asked: Asked) -> Answer {
    use ashpd::desktop::ResponseError;
    use ashpd::desktop::file_chooser::{FileFilter, SelectedFiles};

    let (label, mime, pattern) = &asked.kind;
    let filter = FileFilter::new(label).mimetype(mime).glob(pattern);
    let choices = asked.choices.iter().map(|choice| {
        choice.options.iter().fold(
            ashpd::desktop::file_chooser::Choice::new(&choice.id, &choice.label, &choice.initial),
            |made, (key, label)| made.insert(key, label),
        )
    });
    let request = SelectedFiles::save_file()
        .title(asked.title.as_str())
        .accept_label(asked.accept.as_str())
        .modal(true)
        .current_name(asked.name.as_str())
        .filter(filter.clone())
        .current_filter(filter)
        .choices(choices);
    // A folder that is gone opens where the portal likes.
    let request = match request.current_folder::<&std::path::Path>(Some(asked.folder.as_path())) {
        Ok(request) => request,
        Err(why) => return Answer::Unavailable(why.to_string()),
    };
    let response = match request.send().await {
        Ok(sent) => sent.response(),
        Err(why) => return Answer::Unavailable(why.to_string()),
    };
    match response {
        Ok(files) => {
            let Some(path) = files.uris().first().and_then(|uri| file_path(uri.as_str())) else {
                return Answer::Unavailable("the dialog named no local file".into());
            };
            Answer::Chosen {
                path,
                choices: files.choices().to_vec(),
            }
        }
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => Answer::Cancelled,
        Err(why) => Answer::Unavailable(why.to_string()),
    }
}

/// Opens the dialog and waits for its answer: here there is none.
#[cfg(not(target_os = "linux"))]
pub async fn save(_asked: Asked) -> Answer {
    Answer::Unavailable("no save dialog on this system yet".into())
}

/// The local path of a `file://` URI, its `%XX` escapes undone; `None`
/// for any other URI.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn file_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file:///home/…`, or `file://localhost/home/…`.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        let hex = |b: u8| char::from(b).to_digit(16);
        match (byte, bytes.get(at + 1).copied(), bytes.get(at + 2).copied()) {
            (b'%', Some(high), Some(low)) => {
                let (high, low) = (hex(high)?, hex(low)?);
                out.push((high * 16 + low) as u8);
                at += 3;
            }
            _ => {
                out.push(byte);
                at += 1;
            }
        }
    }
    path_of(out)
}

#[cfg(unix)]
fn path_of(bytes: Vec<u8>) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

#[cfg(not(unix))]
fn path_of(bytes: Vec<u8>) -> Option<PathBuf> {
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_uri_is_its_path_with_its_escapes_undone() {
        assert_eq!(
            file_path("file:///home/me/Diagrams/door%20open.svg"),
            Some(PathBuf::from("/home/me/Diagrams/door open.svg"))
        );
        assert_eq!(
            file_path("file:///tmp/T%C3%BCr.png"),
            Some(PathBuf::from("/tmp/Tür.png"))
        );
        assert_eq!(
            file_path("file://localhost/tmp/a.svg"),
            Some(PathBuf::from("/tmp/a.svg"))
        );
        assert_eq!(file_path("https://example.com/a.svg"), None);
        assert_eq!(file_path("file://server/share/a.svg"), None);
        assert_eq!(file_path("file:///tmp/bad%zz.svg"), None);
    }

    #[test]
    fn an_answer_names_the_key_of_each_choice() {
        let answer = Answer::Chosen {
            path: PathBuf::from("/tmp/a.svg"),
            choices: vec![("look".into(), "dark".into())],
        };
        assert_eq!(answer.choice("look"), Some("dark"));
        assert_eq!(answer.choice("other"), None);
        assert_eq!(Answer::Cancelled.choice("look"), None);
    }
}
