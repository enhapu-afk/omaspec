//! Git, asked for versions: the commits that changed a file, its text in
//! one of them, a folder's files as of a commit (`omaspec diff DIR`), and
//! a file's history with its texts (`omaspec log`).
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
    commits(dir, name)
}

/// `HEAD`, the newest tags and the commits that changed anything in the
/// folder `dir`, as (revision, what to show): what a project is compared
/// since. Empty outside a git repository.
pub fn folder_revisions(dir: &Path) -> Vec<(String, String)> {
    let mut found = commits(dir, std::ffi::OsStr::new("."));
    if found.is_empty() {
        return found;
    }
    let tags = git(dir)
        .args(["tag", "--sort=-creatordate", "--format=%(refname:short)"])
        .output();
    let tags = tags.ok().filter(|o| o.status.success()).map(|o| o.stdout);
    let tags = String::from_utf8_lossy(tags.as_deref().unwrap_or_default()).into_owned();
    let tags = tags
        .lines()
        .filter(|tag| revision(tag).is_ok())
        .take(MAX_TAGS)
        .map(|tag| (tag.to_owned(), format!("tag {tag}")));
    let head = found.remove(0);
    found.splice(0..0, std::iter::once(head).chain(tags));
    found
}

/// Tags offered at most, the newest.
const MAX_TAGS: usize = 5;

/// `HEAD` and the commits that changed `path` (in `dir`), newest first.
fn commits(dir: &Path, path: &std::ffi::OsStr) -> Vec<(String, String)> {
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
        .arg(path)
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

/// The text of `file` as of `rev`; a file whose folder is gone since is
/// asked for from the nearest folder still there.
pub fn show(file: &Path, rev: &str) -> Result<String> {
    revision(rev)?;
    let file = std::path::absolute(file)?;
    let dir = file
        .ancestors()
        .skip(1)
        .find(|d| d.is_dir())
        .context("no folder")?;
    let name = file.strip_prefix(dir)?;
    let name = crate::specs::file_name(name);
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
    let mut wanted: Vec<(String, String)> = Vec::new();
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
            wanted.push((object.to_owned(), path.to_owned()));
        }
    }
    let objects: Vec<&str> = wanted.iter().map(|(o, _)| o.as_str()).collect();
    let texts = blobs(dir, &objects, u64::MAX)?;
    Ok(wanted
        .into_iter()
        .zip(texts)
        .filter_map(|((_, path), text)| Some((path, text?)))
        .collect())
}

/// The texts of the git `objects` in `dir` (whole or abbreviated ids), in
/// their order, through one `git cat-file --batch`, without filters or
/// text conversions; `None` for one larger than `max` bytes.
fn blobs(dir: &Path, objects: &[&str], max: u64) -> Result<Vec<Option<String>>> {
    if objects.is_empty() {
        return Ok(Vec::new());
    }
    let mut child = git(dir)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot run git")?;
    let mut stdin = child.stdin.take().context("no input to git")?;
    let asked: String = objects.iter().map(|o| format!("{o}\n")).collect();
    // Written beside the reading, so neither waits on a full pipe.
    let writer = std::thread::spawn(move || stdin.write_all(asked.as_bytes()));
    let mut reader = BufReader::new(child.stdout.take().context("no output from git")?);
    let mut found = Vec::with_capacity(objects.len());
    let mut header = String::new();
    for object in objects {
        header.clear();
        reader.read_line(&mut header)?;
        // `<object> blob <size>`, or `<object> missing`.
        let mut parts = header.split_whitespace();
        let size = match (parts.next(), parts.next(), parts.next()) {
            (Some(id), Some("blob"), Some(size)) if id.starts_with(object) => {
                size.parse::<u64>()?
            }
            _ => anyhow::bail!("git: cannot read {object} ({})", header.trim()),
        };
        // The text and the newline after it.
        let mut body = Vec::new();
        (&mut reader).take(size + 1).read_to_end(&mut body)?;
        anyhow::ensure!(body.len() as u64 == size + 1, "git: {object} ends early");
        body.pop();
        found.push((size <= max).then(|| String::from_utf8_lossy(&body).into_owned()));
    }
    drop(reader);
    let _ = writer.join();
    let _ = child.wait();
    Ok(found)
}

/// The path of the folder `dir` from its repository's root, `/` after
/// it: empty at the root.
pub fn prefix(dir: &Path) -> Result<String> {
    let out = git(dir)
        .args(["rev-parse", "--show-prefix"])
        .output()
        .context("cannot run git")?;
    anyhow::ensure!(
        out.status.success(),
        "git: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_owned())
}

/// The value of `key` in the repository's own configuration at `dir`;
/// `None` where it is not set.
pub fn local_config(dir: &Path, key: &str) -> Option<String> {
    let out = git(dir)
        .args(["config", "--local", "--get", key])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
}

/// Sets `key` to `value` in the repository's own configuration at `dir`.
pub fn set_local_config(dir: &Path, key: &str, value: &str) -> Result<()> {
    let out = git(dir)
        .args(["config", "--local", key, value])
        .output()
        .context("cannot run git")?;
    anyhow::ensure!(
        out.status.success(),
        "git: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

/// A commit that changed a file.
#[derive(Debug, Clone)]
pub struct FileCommit {
    pub hash: String,
    pub short: String,
    /// `2026-11-05`.
    pub date: String,
    pub subject: String,
    /// Its first parent; `None` for the first commit.
    pub parent: Option<String>,
    /// The file before the commit; `None` where the commit added it.
    pub before: Option<Version>,
    /// The file after the commit; `None` where the commit removed it.
    pub after: Option<Version>,
}

/// A file as one commit has it.
#[derive(Debug, Clone)]
pub struct Version {
    /// From the repository's root, `/` between folders.
    pub path: String,
    /// `None` when it is larger than the reader takes.
    pub text: Option<String>,
}

/// The commits that changed `file`, newest first, following it where it
/// was renamed, at most `count` of them; with the file before and after
/// each, texts of at most `max` bytes. Merges are left out, as `git log`
/// shows no change for them.
pub fn file_log(file: &Path, count: Option<usize>, max: u64) -> Result<Vec<FileCommit>> {
    let file = std::path::absolute(file)?;
    // A file removed with its folder is asked for from the nearest folder
    // still there.
    let dir = file
        .ancestors()
        .skip(1)
        .find(|d| d.is_dir())
        .context("no folder")?;
    let name = file.strip_prefix(dir)?;
    let mut command = git(dir);
    command.args([
        "log",
        "--follow",
        // Abbreviated as `%h` is; git abbreviates them so they stay unique.
        "--raw",
        "-z",
        "--no-color",
        "--date=short",
        "--format=%x01%H%x09%h%x09%ad%x09%P%x09%s",
    ]);
    if let Some(count) = count {
        command.arg(format!("-n{count}"));
    }
    let out = command
        .arg("--")
        .arg(name)
        .output()
        .context("cannot run git")?;
    anyhow::ensure!(
        out.status.success(),
        "git: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let mut commits: Vec<(FileCommit, Option<String>, Option<String>)> = Vec::new();
    // `\x01<header>\0`, then `\n:<modes> <before> <after> <status>\0<path>\0`, a
    // rename with the path before and after.
    let text = String::from_utf8_lossy(&out.stdout);
    let mut tokens = text.split('\0');
    while let Some(token) = tokens.next() {
        let token = token.trim_start_matches('\n');
        if let Some(header) = token.strip_prefix('\x01') {
            let mut parts = header.splitn(5, '\t');
            let mut part = || parts.next().unwrap_or_default().to_owned();
            let (hash, short, date, parents, subject) = (part(), part(), part(), part(), part());
            let commit = FileCommit {
                hash,
                short,
                date,
                subject,
                parent: parents.split_whitespace().next().map(str::to_owned),
                before: None,
                after: None,
            };
            commits.push((commit, None, None));
        } else if let Some(meta) = token.strip_prefix(':') {
            let mut parts = meta.split_whitespace().skip(2);
            let (Some(before), Some(after), Some(status)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let first = tokens.next().unwrap_or_default().to_owned();
            let (from, to) = match status.starts_with(['R', 'C']) {
                true => (first, tokens.next().unwrap_or_default().to_owned()),
                false => (first.clone(), first),
            };
            let Some((commit, b, a)) = commits.last_mut() else {
                continue;
            };
            let blob = |id: &str| (!id.chars().all(|c| c == '0')).then(|| id.to_owned());
            if let Some(id) = blob(before) {
                commit.before = Some(Version {
                    path: from,
                    text: None,
                });
                *b = Some(id);
            }
            if let Some(id) = blob(after) {
                commit.after = Some(Version {
                    path: to,
                    text: None,
                });
                *a = Some(id);
            }
        }
    }
    let ids: Vec<&str> = commits
        .iter()
        .flat_map(|(_, b, a)| [b.as_deref(), a.as_deref()])
        .flatten()
        .collect();
    let mut texts = blobs(dir, &ids, max)?.into_iter();
    Ok(commits
        .into_iter()
        .map(|(mut commit, b, a)| {
            if let (Some(version), Some(_)) = (commit.before.as_mut(), b) {
                version.text = texts.next().flatten();
            }
            if let (Some(version), Some(_)) = (commit.after.as_mut(), a) {
                version.text = texts.next().flatten();
            }
            commit
        })
        .collect())
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
        let log = file_log(&file, None, u64::MAX).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].subject, "signed");
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
    fn a_file_s_log_follows_it_and_reads_each_version() {
        let dir = std::env::temp_dir().join(format!("omaspec-git-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("specs")).unwrap();
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
        write("specs/door.puml", "one\n");
        assert!(git(&["add", "."]) && git(&["commit", "-qm", "add"]));
        write("specs/door.puml", "two\n");
        assert!(git(&["commit", "-qam", "change\tit"]));
        assert!(git(&["mv", "specs/door.puml", "specs/front door.puml"]));
        assert!(git(&["commit", "-qm", "rename"]));
        assert!(git(&["rm", "-q", "specs/front door.puml"]) && git(&["commit", "-qm", "gone"]));

        let log = file_log(&dir.join("specs/front door.puml"), None, u64::MAX).unwrap();
        let subjects: Vec<&str> = log.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["gone", "rename", "change\tit", "add"]);
        let text = |v: &Option<Version>| v.as_ref().and_then(|v| v.text.clone());
        let path = |v: &Option<Version>| v.as_ref().map(|v| v.path.clone());
        // Removed: only before; renamed: both paths; added: only after.
        assert_eq!(text(&log[0].before).as_deref(), Some("two\n"));
        assert!(log[0].after.is_none());
        assert_eq!(path(&log[1].before).as_deref(), Some("specs/door.puml"));
        assert_eq!(
            path(&log[1].after).as_deref(),
            Some("specs/front door.puml")
        );
        assert_eq!(text(&log[2].before).as_deref(), Some("one\n"));
        assert!(log[3].before.is_none() && log[3].parent.is_none());
        assert!(log[0].short.len() < log[0].hash.len());
        // At most two, and none of a text over the size given.
        let short = file_log(&dir.join("specs/front door.puml"), Some(2), 2).unwrap();
        assert_eq!(short.len(), 2);
        assert!(text(&short[1].after).is_none());
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

        // What the project compares since: HEAD, the tags, then its commits.
        assert!(git(&["tag", "v1", "HEAD~1"]));
        let revs = folder_revisions(&specs);
        let labels: Vec<&str> = revs.iter().map(|(_, label)| label.as_str()).collect();
        assert!(
            labels[0].starts_with("HEAD · ") && labels[0].ends_with("two"),
            "{labels:?}"
        );
        assert_eq!(labels[1], "tag v1");
        assert_eq!(revs[1].0, "v1");
        assert!(labels[2].ends_with("one"), "{labels:?}");
        assert_eq!(labels.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
