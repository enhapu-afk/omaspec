//! `omaspec __sandbox SCRATCH -- COMMAND ARGS…` runs COMMAND (the Java
//! PlantUML) confined with Landlock: it may read and execute files only in
//! the system's folders, its own folder, its Java and the fonts (not the
//! home folder, so not `~/.ssh`, and not the `/proc` entries of other
//! processes, which hold their environment), write only below SCRATCH,
//! open no TCP connection, and, where the kernel offers it, reach no
//! abstract Unix socket and signal no process outside.
//!
//! Without `unsafe` there is no hook between fork and exec, so Omaspec
//! starts itself for this, confines itself and becomes COMMAND. On a kernel
//! without Landlock the command runs anyway, with PlantUML's own SANDBOX
//! profile as the only fence.

use std::env;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result};
use landlock::{
    ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, RestrictionStatus, Ruleset,
    RulesetAttr, RulesetCreatedAttr, RulesetStatus, Scope, path_beneath_rules,
};
use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};

/// The newest Landlock ABI asked for; older kernels give what they can.
const ABI_WANTED: ABI = ABI::V6;

/// The most files the command may hold open. Java closes every descriptor
/// up to this limit one by one in each process it starts (Graphviz, for
/// PlantUML), because the child cannot list its `/proc/self/fd` here; with
/// Omarchy's 524,288 that cost 160 ms a class diagram.
const OPEN_FILES: u64 = 4096;

/// The system's folders and files a JVM, Graphviz and fontconfig read.
/// `/proc/self` is resolved when the rules are made, so it is this
/// process, which becomes the command. Paths that are not there are left
/// out.
const SYSTEM: [&str; 19] = [
    "/usr",
    "/etc",
    "/opt",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/sys",
    "/var/cache/fontconfig",
    "/dev/random",
    "/dev/urandom",
    "/dev/zero",
    "/proc/self",
    "/proc/cgroups",
    "/proc/cpuinfo",
    "/proc/filesystems",
    "/proc/meminfo",
    "/proc/stat",
    "/proc/sys",
];

/// Confines this process (and what it becomes) as described above.
pub fn confine(scratch: &Path, command: &Path) -> Result<RestrictionStatus> {
    let abi = ABI_WANTED;
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(abi))?
        .handle_access(AccessNet::from_all(abi))?
        .scope(Scope::from_all(abi))?
        .create()?
        // Read and execute: the JVM, its libraries, fonts.
        .add_rules(path_beneath_rules(
            readable(command),
            AccessFs::from_read(abi),
        ))?
        // Write only in the scratch folder, and to /dev/null.
        .add_rules(path_beneath_rules([scratch], AccessFs::from_all(abi)))?
        .add_rules(path_beneath_rules(["/dev/null"], AccessFs::from_all(abi)))?
        .restrict_self()?;
    Ok(status)
}

/// Where `command` may read and execute: [`SYSTEM`], the folder it lies
/// in (links followed), the Java of `JAVA_HOME` and of `PATH`, and the
/// user's fonts and font settings. A command right in the home folder
/// does not open it.
fn readable(command: &Path) -> Vec<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let home = home.as_deref();
    let xdg = |var: &str, default: &str| {
        env::var_os(var)
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(default)))
    };
    let mut paths: Vec<PathBuf> = SYSTEM.iter().map(PathBuf::from).collect();
    paths.extend(
        [
            xdg("XDG_DATA_HOME", ".local/share").map(|d| d.join("fonts")),
            home.map(|h| h.join(".fonts")),
            xdg("XDG_CONFIG_HOME", ".config").map(|d| d.join("fontconfig")),
            xdg("XDG_CACHE_HOME", ".cache").map(|d| d.join("fontconfig")),
        ]
        .into_iter()
        .flatten(),
    );
    let java = env::var_os("JAVA_HOME")
        .map(|jdk| PathBuf::from(jdk).join("bin/java"))
        .into_iter()
        .chain(find("java"));
    paths.extend(java.filter_map(|java| jdk(&java)));
    let program = if command.components().count() > 1 {
        Some(command.to_path_buf())
    } else {
        find(command.as_os_str())
    };
    paths.extend(
        program
            .and_then(|p| p.canonicalize().ok())
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|dir| home.is_none_or(|h| !h.starts_with(dir))),
    );
    paths
}

/// The first `name` in `PATH`.
fn find(name: impl AsRef<std::ffi::OsStr>) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(name.as_ref()))
        .find(|path| path.is_file())
}

/// The JDK a `java` belongs to (links followed): the folder above its
/// `bin`, when that holds the `release` file every JDK has.
fn jdk(java: &Path) -> Option<PathBuf> {
    let real = java.canonicalize().ok()?;
    let bin = real.parent()?;
    let root = bin.parent()?;
    (bin.file_name()? == "bin" && root.join("release").is_file()).then(|| root.to_path_buf())
}

/// The `__sandbox` command: confine, then become `command`.
pub fn run(scratch: &Path, command: &[OsString]) -> Result<ExitCode> {
    let (program, args) = command.split_first().context("no command to run")?;
    let status =
        confine(scratch, Path::new(program)).context("cannot set up the Landlock sandbox")?;
    if status.ruleset == RulesetStatus::NotEnforced {
        eprintln!("omaspec: this kernel has no Landlock; running PlantUML unconfined");
    }
    limit_open_files();
    // Only returns when the command could not be started.
    let err = Command::new(program).args(args).exec();
    Err(err).with_context(|| format!("cannot run {}", Path::new(program).display()))
}

/// Lowers the open-file limit to [`OPEN_FILES`] when it is higher; the
/// hard limit stays.
fn limit_open_files() {
    let limit = getrlimit(Resource::Nofile);
    if limit.current.is_none_or(|current| current > OPEN_FILES) {
        let lowered = Rlimit {
            current: Some(OPEN_FILES),
            maximum: limit.maximum,
        };
        // A limit that stays high only makes Graphviz slower.
        let _ = setrlimit(Resource::Nofile, lowered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_jdk_a_java_belongs_to() {
        let root = env::temp_dir().join(format!("omaspec-jdk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("jdk/bin")).unwrap();
        std::fs::write(root.join("jdk/bin/java"), "").unwrap();
        std::fs::create_dir_all(root.join("shims")).unwrap();
        std::os::unix::fs::symlink(root.join("jdk/bin/java"), root.join("shims/java")).unwrap();
        // Not a JDK yet: no `release`.
        assert_eq!(jdk(&root.join("shims/java")), None);
        std::fs::write(root.join("jdk/release"), "JAVA_VERSION=\"21\"\n").unwrap();
        let found = jdk(&root.join("shims/java")).unwrap();
        assert_eq!(found, root.join("jdk").canonicalize().unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }
}
