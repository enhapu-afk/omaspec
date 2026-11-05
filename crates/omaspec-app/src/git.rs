//! Git, asked for versions: the commits that changed a file, its text in
//! one of them, and a folder's files as of a commit (`omaspec diff DIR`).
//! Git starts nothing a repository's own configuration names ([`git`]),
//! and a revision is never taken for an option.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Commits offered at most.
const MAX_COMMITS: usize = 30;

/// Git in `dir`, kept from starting programs a repository names.
///
/// A repository's own `.git/config` comes with a folder someone hands over
/// (an archive, a stick), and git trusts it. `log.showSignature` with
/// `gpg.program` would make `git log` run any program on a signed commit;
/// the others switch off what could run one too: a file system monitor, a
/// pager, external diffs and text conversions, hooks. No prompt waits for a
/// password, no lock is taken, and a partial clone fetches nothing
/// missing: Omaspec goes to no network.
fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir);
    for setting in [
        "log.showSignature=false",
        "core.fsmonitor=false",
        "core.pager=cat",
        "core.hooksPath=/dev/null",
        "diff.external=",
    ] {
        command.args(["-c", setting]);
    }
    command
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1");
    command
}

/// `HEAD` and the commits that changed `file`, newest first, as
/// (revision, what to show); empty outside a git repository.
pub fn revisions(file: &Path) -> Vec<(String, String)> {
    let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else {
        return Vec::new();
    };
    let out = git(dir)
        .args([
            "log",
            "--no-show-signature",
            "--no-color",
            &format!("-n{MAX_COMMITS}"),
            "--format=%h%x09%ad%x09%s",
            "--date=short",
            "--",
        ])
        .arg(name)
        .output();
    let Some(out) = out.ok().filter(|o| o.status.success()) else {
        return Vec::new();
    };
    let log = String::from_utf8_lossy(&out.stdout);
    let mut found: Vec<(String, String)> = log
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (rev, date, subject) = (parts.next()?, parts.next()?, parts.next()?);
            Some((rev.to_owned(), format!("{rev} {date} {subject}")))
        })
        .collect();
    if let Some((_, label)) = found.first_mut() {
        *label = format!("HEAD · {label}");
    }
    found
}

/// The text of `file` as of `rev`.
pub fn show(file: &Path, rev: &str) -> Result<String> {
    revision(rev)?;
    let file = std::path::absolute(file)?;
    let dir = file.parent().context("no folder")?;
    let name = file.file_name().context("no file name")?.to_string_lossy();
    // `./` makes the path relative to `dir`, not to the repository.
    let out = git(dir)
        .args([
            "show",
            "--no-color",
            "--no-textconv",
            "--no-ext-diff",
            &format!("{rev}:./{name}"),
        ])
        .output()
        .context("cannot run git")?;
    anyhow::ensure!(
        out.status.success(),
        "git: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Refuses what is no revision: an option such as `--output=…` would make
/// git write.
fn revision(rev: &str) -> Result<()> {
    let known = |c: char| c.is_ascii_alphanumeric() || "_.^~/@{}-".contains(c);
    anyhow::ensure!(
        !rev.is_empty() && !rev.starts_with('-') && rev.chars().all(known),
        "`{rev}` is no git revision"
    );
    Ok(())
}

/// The files under `dir` as of `rev` that `keep` takes by their paths
/// (relative to `dir`, `/` between folders) and their sizes, with their
/// texts. Links and submodules are no files. Empty when `dir` was not
/// there yet.
pub fn files_at(
    dir: &Path,
    rev: &str,
    keep: impl Fn(&str, u64) -> bool,
) -> Result<Vec<(String, String)>> {
    revision(rev)?;
    // Run in `dir`, ls-tree lists what lies under it, by paths from there.
    let out = git(dir)
        .args(["ls-tree", "-r", "-z", "-l", rev])
        .output()
        .context("cannot run git")?;
    anyhow::ensure!(
        out.status.success(),
        "git: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    // `<mode> <type> <object> <size>\t<path>`, each ended by a zero.
    let mut wanted: Vec<(String, u64, String)> = Vec::new();
    for record in out.stdout.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        let record = String::from_utf8_lossy(record);
        let Some((meta, path)) = record.split_once('\t') else {
            continue;
        };
        let mut parts = meta.split_whitespace();
        let (Some(mode), Some("blob"), Some(object), Some(size)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(size) = size.parse::<u64>() else {
            continue;
        };
        if mode != "120000" && keep(path, size) {
            wanted.push((object.to_owned(), size, path.to_owned()));
        }
    }
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    // One git for all of them, without filters or text conversions.
    let mut child = git(dir)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot run git")?;
    let mut stdin = child.stdin.take().context("no input to git")?;
    let objects: String = wanted.iter().map(|(o, _, _)| format!("{o}\n")).collect();
    // Written beside the reading, so neither waits on a full pipe.
    let writer = std::thread::spawn(move || stdin.write_all(objects.as_bytes()));
    let mut reader = BufReader::new(child.stdout.take().context("no output from git")?);
    let mut found = Vec::with_capacity(wanted.len());
    let mut header = String::new();
    for (object, size, path) in wanted {
        header.clear();
        reader.read_line(&mut header)?;
        // `<object> blob <size>`, or `<object> missing`.
        let mut parts = header.split_whitespace();
        let same = parts.next() == Some(object.as_str())
            && parts.next() == Some("blob")
            && parts.next() == Some(size.to_string().as_str());
        anyhow::ensure!(same, "git: cannot read {path} ({})", header.trim());
        let mut body = vec![0; usize::try_from(size)? + 1];
        reader.read_exact(&mut body)?;
        body.pop();
        found.push((path, String::from_utf8_lossy(&body).into_owned()));
    }
    drop(reader);
    let _ = writer.join();
    let _ = child.wait();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repository handed over with a `.git/config` that names programs:
    /// listing and reading its versions starts none of them.
    #[test]
    fn a_prepared_repository_starts_nothing() {
        let dir = std::env::temp_dir().join(format!("omaspec-git-trap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .output()
                .is_ok_and(|o| o.status.success())
        };
        if !run(&["init", "-q"]) {
            eprintln!("skipped: no git");
            return;
        }
        let file = dir.join("door.puml");
        std::fs::write(&file, "@startuml\nA -> B : x\n@enduml\n").unwrap();
        std::fs::write(dir.join(".gitattributes"), "*.puml diff=trap filter=trap\n").unwrap();
        assert!(run(&["add", "."]) && run(&["commit", "-qm", "one", "--no-gpg-sign"]));
        // A commit that looks signed, touching the file.
        std::fs::write(&file, "@startuml\nA -> B : y\n@enduml\n").unwrap();
        assert!(run(&["add", "door.puml"]));
        let out = |args: &[&str]| {
            let o = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .unwrap();
            String::from_utf8_lossy(&o.stdout).trim().to_owned()
        };
        let (tree, parent) = (out(&["write-tree"]), out(&["rev-parse", "HEAD"]));
        let commit = format!(
            "tree {tree}\nparent {parent}\nauthor t <t@t> 1700000000 +0000\ncommitter t <t@t> 1700000000 +0000\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n iQEzBAABCAAdFiEE\n -----END PGP SIGNATURE-----\n\nsigned\n"
        );
        std::fs::write(dir.join("commit.txt"), commit).unwrap();
        let signed = out(&["hash-object", "-t", "commit", "-w", "commit.txt"]);
        assert!(run(&["update-ref", "HEAD", &signed]));
        let trap = dir.join("trap.sh");
        let marker = dir.join("ran");
        std::fs::write(
            &trap,
            format!("#!/bin/sh\necho \"$0 $*\" >> {}\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&trap, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let trap = trap.display().to_string();
        for (key, value) in [
            ("log.showSignature", "true"),
            ("gpg.program", trap.as_str()),
            ("core.fsmonitor", trap.as_str()),
            ("core.pager", trap.as_str()),
            ("diff.trap.textconv", trap.as_str()),
            ("diff.external", trap.as_str()),
            ("filter.trap.smudge", trap.as_str()),
            ("filter.trap.textconv", trap.as_str()),
        ] {
            assert!(run(&["config", key, value]));
        }
        assert_eq!(revisions(&file).len(), 2);
        assert!(show(&file, "HEAD").unwrap().contains("A -> B : y"));
        let files = files_at(&dir, "HEAD", |path, _| path.ends_with(".puml")).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].1.contains("A -> B : y"));
        assert!(
            !marker.exists(),
            "git started {}",
            std::fs::read_to_string(&marker).unwrap_or_default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_versions_from_git() {
        let dir = std::env::temp_dir().join(format!("omaspec-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        let file = dir.join("flow.mmd");
        assert!(revisions(&file).is_empty(), "no repository yet");
        git(&["init", "-q"]);
        std::fs::write(&file, "flowchart LR\n  A --> B\n").unwrap();
        git(&["add", "flow.mmd"]);
        git(&["commit", "-q", "-m", "first"]);
        std::fs::write(&file, "flowchart LR\n  A --> C\n").unwrap();
        git(&["commit", "-q", "-am", "second"]);
        std::fs::write(&file, "flowchart LR\n  A --> D\n").unwrap();

        let revs = revisions(&file);
        assert_eq!(revs.len(), 2);
        assert!(revs[0].1.starts_with("HEAD · ") && revs[0].1.ends_with("second"));
        assert_eq!(
            show(&file, &revs[0].0).unwrap(),
            "flowchart LR\n  A --> C\n"
        );
        assert_eq!(
            show(&file, &revs[1].0).unwrap(),
            "flowchart LR\n  A --> B\n"
        );
        assert!(show(&file, "nosuchrev").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_a_folder_as_of_a_commit() {
        let dir = std::env::temp_dir().join(format!("omaspec-git-tree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("specs/rooms")).unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .output()
                .is_ok_and(|o| o.status.success())
        };
        if !git(&["init", "-q"]) {
            eprintln!("skipped: no git");
            return;
        }
        let write = |file: &str, text: &str| std::fs::write(dir.join(file), text).unwrap();
        write("specs/door.puml", "@startuml\n[*] --> Closed\n@enduml\n");
        write("specs/rooms/lamp.mmd", "stateDiagram-v2\n[*] --> Off\n");
        write("specs/notes.txt", "not a diagram");
        write("outside.puml", "@startuml\nA -> B\n@enduml\n");
        assert!(git(&["add", "."]) && git(&["commit", "-qm", "one"]));
        std::os::unix::fs::symlink("door.puml", dir.join("specs/link.puml")).unwrap();
        write("specs/door.puml", "@startuml\n[*] --> Open\n@enduml\n");
        assert!(git(&["add", "."]) && git(&["commit", "-qm", "two"]));

        let specs = dir.join("specs");
        let all = |_: &str, _: u64| true;
        let now = files_at(&specs, "HEAD", all).unwrap();
        let paths: Vec<&str> = now.iter().map(|(p, _)| p.as_str()).collect();
        // Paths from the folder, nothing outside it, no link.
        assert_eq!(paths, ["door.puml", "notes.txt", "rooms/lamp.mmd"]);
        assert!(now[0].1.contains("Open"));
        let before = files_at(&specs, "HEAD~1", |p, _| p.ends_with(".puml")).unwrap();
        assert_eq!(before.len(), 1);
        assert!(before[0].1.contains("Closed"));
        // By size too.
        assert!(
            files_at(&specs, "HEAD", |_, size| size > 1000)
                .unwrap()
                .is_empty()
        );
        // A folder that was not there yet has no files then.
        std::fs::create_dir_all(dir.join("later")).unwrap();
        assert!(
            files_at(&dir.join("later"), "HEAD", all)
                .unwrap()
                .is_empty()
        );
        assert!(files_at(&specs, "--output=x", all).is_err());
        assert!(files_at(&specs, "nosuchrev", all).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
