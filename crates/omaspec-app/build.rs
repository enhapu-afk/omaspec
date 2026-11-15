//! The version Omaspec tells (`--version`, the MCP server): the
//! workspace's, and past it the commit it was built from, as
//! `0.9.8+3b94dc3`, unless that commit is the release's own tag. Built
//! from a source archive, without git, it is the version alone.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let told = match git(&["rev-parse", "--short=7", "HEAD"]) {
        Some(_)
            if git(&["describe", "--tags", "--exact-match", "HEAD"])
                == Some(format!("v{version}")) =>
        {
            version
        }
        Some(commit) => format!("{version}+{commit}"),
        None => version,
    };
    println!("cargo:rustc-env=OMASPEC_VERSION={told}");
    // Told again once the commit, a branch or a tag moves.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        let dir = PathBuf::from(dir);
        for watched in ["HEAD", "refs", "packed-refs"] {
            println!("cargo:rerun-if-changed={}", dir.join(watched).display());
        }
    } else {
        println!("cargo:rerun-if-changed=build.rs");
    }
}

/// What `git args` says, trimmed; `None` without git, outside a
/// repository, or when it fails.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let said = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !said.is_empty()).then_some(said)
}
