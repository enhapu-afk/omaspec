//! Reading diagram sources and writing files without leaving half-written ones.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Sources larger than this are refused; no diagram is that big.
pub const MAX_SOURCE: u64 = 16 << 20;

/// A diagram source and where it came from.
pub struct Input {
    /// `None` for standard input.
    pub path: Option<PathBuf>,
    pub text: String,
}

impl Input {
    /// The name to show in messages.
    pub fn name(&self) -> String {
        match &self.path {
            Some(p) => p.display().to_string(),
            None => "<stdin>".into(),
        }
    }

    pub fn file_name(&self) -> Option<&str> {
        self.path.as_deref()?.file_name()?.to_str()
    }
}

/// The given files, or standard input for none or `-`.
pub fn read_inputs(files: &[PathBuf]) -> Result<Vec<Input>> {
    if files.is_empty() {
        return Ok(vec![read_input(None)?]);
    }
    files
        .iter()
        .map(|f| read_input((f.as_os_str() != "-").then_some(f.as_path())))
        .collect()
}

pub fn read_input(path: Option<&Path>) -> Result<Input> {
    let text = match path {
        Some(p) => {
            let size = fs::metadata(p)
                .with_context(|| p.display().to_string())?
                .len();
            anyhow::ensure!(size <= MAX_SOURCE, "{}: larger than 16 MB", p.display());
            fs::read_to_string(p).with_context(|| p.display().to_string())?
        }
        None => {
            let mut text = String::new();
            io::stdin()
                .take(MAX_SOURCE + 1)
                .read_to_string(&mut text)
                .context("reading standard input")?;
            anyhow::ensure!(
                text.len() as u64 <= MAX_SOURCE,
                "standard input larger than 16 MB"
            );
            text
        }
    };
    Ok(Input {
        path: path.map(Path::to_owned),
        text,
    })
}

/// Replaces `path` with `contents` atomically: a temporary file in the same
/// directory, flushed, then renamed over the original. Keeps the
/// original's permissions.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .context("not a file path")?
        .to_string_lossy();
    let tmp = dir.join(format!(".{name}.omaspec-{}", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = fs::File::create_new(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.with_context(|| format!("writing {}", path.display()))
}

/// Puts `bytes` on the Wayland clipboard with `wl-copy`, which keeps
/// serving it after Omaspec exits.
pub fn copy_to_clipboard(bytes: &[u8], mime: &str) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(Stdio::piped())
        .spawn()
        .context("cannot run wl-copy (package wl-clipboard)")?;
    child
        .stdin
        .take()
        .context("wl-copy has no input")?
        .write_all(bytes)?;
    let status = child.wait()?;
    anyhow::ensure!(status.success(), "wl-copy failed: {status}");
    Ok(())
}
