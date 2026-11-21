//! The Java PlantUML as a fallback, kept running between diagrams.
//!
//! Starting a JVM takes a second or more, so one `plantuml -pipe` process
//! serves all diagrams: Omaspec writes a diagram to its standard input
//! and reads the SVG up to a delimiter line. A process that hangs is killed
//! after a timeout, one that writes more than [`MAX_ANSWER`] for a diagram
//! as soon as it has; one that dies is started again with the next diagram.
//!
//! The process runs with `PLANTUML_SECURITY_PROFILE=SANDBOX`: no `!include`
//! of local files or URLs, no network. `JAVA_TOOL_OPTIONS` ends with
//! `-DPLANTUML_SECURITY_PROFILE=SANDBOX`, after any inherited value, which
//! is the profile PlantUML reads. It starts with little of Omaspec's
//! environment ([`KEPT_ENV`]), so no token reaches it. With a
//! [`Config::sandbox`] it is also started confined: it can write only in a
//! scratch folder of its own, removed when it ends, and the JVM keeps its
//! temporary files there. The sandbox says first what the fence lacks on
//! this kernel ([`SANDBOX_REPORT`]), and each picture it draws says it in
//! turn ([`crate::Rendered::fence`]).

use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{Config, Engine, RenderError};

/// The most PlantUML may write for one diagram. Its largest pictures take
/// a few megabytes; a diagram that makes it write more is stopped as a
/// hang is, before its answer fills the memory.
const MAX_ANSWER: usize = 64 << 20;

/// The first line `omaspec __sandbox` writes on standard error, before it
/// becomes PlantUML: this, a space, and what the fence lacks on this
/// kernel, nothing when it holds whole.
pub const SANDBOX_REPORT: &str = "omaspec-sandbox:";

/// The most read of the sandbox's first line, and of what follows it
/// when it cannot start.
const MAX_REPORT: u64 = 4096;

/// What the reader of PlantUML's standard output hands on.
enum Answer {
    Line(String),
    /// The answer to the diagram grew past [`MAX_ANSWER`]; nothing of it
    /// is read on.
    TooLarge,
}

pub struct JavaPlantUml {
    command: PathBuf,
    sandbox: Option<PathBuf>,
    timeout: Duration,
    start_timeout: Duration,
    process: Option<Process>,
}

struct Process {
    child: Child,
    /// The confined process's only writable folder.
    scratch: Option<PathBuf>,
    stdin: ChildStdin,
    lines: Receiver<Answer>,
    delimiter: String,
    /// Diagrams served so far; the first one includes the JVM start.
    served: usize,
    /// What its sandbox said the fence lacks on this kernel.
    fence: Option<String>,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(scratch) = &self.scratch {
            let _ = std::fs::remove_dir_all(scratch);
        }
    }
}

impl JavaPlantUml {
    /// The configured `plantuml` command, or the one in `PATH`.
    pub fn find(config: &Config) -> Option<Self> {
        let command = match &config.plantuml {
            Some(path) => is_executable(path).then(|| path.clone())?,
            None => env::split_paths(&env::var_os("PATH")?)
                .map(|dir| dir.join("plantuml"))
                .find(|p| is_executable(p))?,
        };
        Some(Self {
            command,
            sandbox: config.sandbox.clone(),
            timeout: config.java_timeout,
            start_timeout: config.java_start_timeout,
            process: None,
        })
    }

    pub fn command(&self) -> &Path {
        &self.command
    }

    /// What the fence around the running PlantUML lacks on this kernel, as
    /// its sandbox said when it started; `None` while it holds whole, and
    /// without a sandbox.
    pub fn fence(&self) -> Option<&str> {
        self.process.as_ref()?.fence.as_deref()
    }

    /// The SVG for a PlantUML source. Lines before its first `@start…` are
    /// left out; a source without one is wrapped in `@startuml`/`@enduml`.
    pub fn render(&mut self, source: &str) -> Result<String, RenderError> {
        let (block, first_line) = first_block(source);
        let lines = self.exchange(&block)?;
        let (error, svg) = parse_output(lines);
        match error {
            Some((reported, message)) => Err(RenderError::Diagram {
                engine: Engine::JavaPlantUml,
                // PlantUML counts from 0 within the block it was given,
                // its `@start…` line being 0.
                line: reported.and_then(|r| (first_line + r as isize).try_into().ok()),
                message,
            }),
            None if svg.trim_start().starts_with('<') => Ok(svg),
            None => Err(engine_error("PlantUML returned no SVG")),
        }
    }

    /// Sends one diagram and collects the lines before the delimiter.
    fn exchange(&mut self, block: &str) -> Result<Vec<String>, RenderError> {
        if self.process.is_none() {
            self.process = Some(self.spawn()?);
        }
        let process = self.process.as_mut().expect("just started");
        let timeout = if process.served == 0 {
            self.start_timeout
        } else {
            self.timeout
        };

        let sent = process
            .stdin
            .write_all(block.as_bytes())
            .and_then(|()| process.stdin.flush());
        if sent.is_err() {
            self.process = None;
            return Err(engine_error(
                "PlantUML stopped; it starts again with the next diagram",
            ));
        }

        let deadline = Instant::now() + timeout;
        let mut lines = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match process.lines.recv_timeout(left) {
                // An SVG ends without a line break, so the delimiter
                // follows its `</svg>` on the same line.
                Ok(Answer::Line(mut line)) if line.ends_with(&process.delimiter) => {
                    line.truncate(line.len() - process.delimiter.len());
                    if !line.is_empty() {
                        lines.push(line);
                    }
                    break;
                }
                Ok(Answer::Line(line)) => lines.push(line),
                Ok(Answer::TooLarge) => {
                    self.process = None;
                    return Err(engine_error(&format!(
                        "PlantUML wrote more than {} MB for this diagram and was stopped",
                        MAX_ANSWER >> 20
                    )));
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.process = None;
                    return Err(engine_error(&format!(
                        "PlantUML took longer than {} s and was stopped",
                        timeout.as_secs()
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.process = None;
                    return Err(engine_error(
                        "PlantUML stopped; it starts again with the next diagram",
                    ));
                }
            }
        }
        process.served += 1;
        Ok(lines)
    }

    fn spawn(&self) -> Result<Process, RenderError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let delimiter = format!("__omaspec_{}_{nonce:x}__", std::process::id());
        let (mut command, scratch) = match &self.sandbox {
            Some(launcher) => {
                let scratch = env::temp_dir()
                    .join(format!("omaspec-plantuml-{}-{nonce:x}", std::process::id()));
                private_dir(&scratch).map_err(|e| {
                    engine_error(&format!("cannot create {}: {e}", scratch.display()))
                })?;
                let mut command = kept_env(Command::new(launcher));
                command
                    .arg("__sandbox")
                    .arg(&scratch)
                    .arg("--")
                    .arg(&self.command)
                    .env("TMPDIR", &scratch);
                (command, Some(scratch))
            }
            None => (kept_env(Command::new(&self.command)), None),
        };
        // PlantUML reads the system property before the environment
        // variable, so the profile is the last option, after anything
        // inherited. The scratch folder, when there is one, holds the
        // JVM's temporary files, and the JVM keeps the open-file limit the
        // sandbox lowered.
        let inherited =
            env::var_os("JAVA_TOOL_OPTIONS").map(|value| value.to_string_lossy().into_owned());
        command.env(
            "JAVA_TOOL_OPTIONS",
            java_tool_options(inherited.as_deref(), scratch.as_deref()),
        );
        let spawned = command
            .args([
                "-pipe",
                "-tsvg",
                "-charset",
                "UTF-8",
                "-nometadata",
                "-pipeNoStderr",
            ])
            .args(["-pipedelimitor", &delimiter])
            .env("PLANTUML_SECURITY_PROFILE", "SANDBOX")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if scratch.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                if let Some(scratch) = &scratch {
                    let _ = std::fs::remove_dir_all(scratch);
                }
                return Err(engine_error(&format!(
                    "cannot start {}: {e}",
                    self.command.display()
                )));
            }
        };
        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take();
        let (tx, lines) = mpsc::channel();
        let end = delimiter.clone();
        // Dropped on an error from here on: the process ends, its scratch
        // folder goes.
        let mut process = Process {
            child,
            scratch,
            stdin,
            lines,
            delimiter,
            served: 0,
            fence: None,
        };
        thread::Builder::new()
            .name("plantuml-reader".into())
            .spawn(move || read_answers(stdout, &end, &tx))
            .map_err(|e| engine_error(&e.to_string()))?;
        if let Some(stderr) = stderr {
            process.fence = sandbox_report(stderr, self.start_timeout)?;
        }
        Ok(process)
    }
}

/// What the fence lacks, as the sandbox reports it on `stderr` before it
/// becomes PlantUML. What is written there after it (the JVM writes there
/// unasked) is read and dropped. A sandbox that writes something else
/// first did not start, and says why.
fn sandbox_report(stderr: ChildStderr, timeout: Duration) -> Result<Option<String>, RenderError> {
    let (tx, report) = mpsc::channel();
    thread::Builder::new()
        .name("plantuml-errors".into())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut first = Vec::new();
            let _ = (&mut reader).take(MAX_REPORT).read_until(b'\n', &mut first);
            let first = String::from_utf8_lossy(&first).into_owned();
            if first.starts_with(SANDBOX_REPORT) {
                let _ = tx.send(Ok(first));
                let _ = std::io::copy(&mut reader, &mut std::io::sink());
            } else {
                let mut rest = Vec::new();
                let _ = (&mut reader).take(MAX_REPORT).read_to_end(&mut rest);
                let _ = tx.send(Err(first + &String::from_utf8_lossy(&rest)));
            }
        })
        .map_err(|e| engine_error(&e.to_string()))?;
    match report.recv_timeout(timeout) {
        Ok(Ok(line)) => {
            let lacks = line[SANDBOX_REPORT.len()..].trim();
            Ok((!lacks.is_empty()).then(|| lacks.to_owned()))
        }
        Ok(Err(said)) if !said.trim().is_empty() => Err(engine_error(&format!(
            "its sandbox did not start: {}",
            said.split_whitespace().collect::<Vec<_>>().join(" ")
        ))),
        _ => Err(engine_error("its sandbox did not start")),
    }
}

/// Hands on PlantUML's output line by line, each without its line break,
/// until it ends, is not UTF-8, or the answer to one diagram (the lines up
/// to the one that ends with `delimiter`) grows past [`MAX_ANSWER`]. A
/// line is read no further than that either.
fn read_answers(stdout: impl Read, delimiter: &str, tx: &Sender<Answer>) {
    let mut reader = BufReader::new(stdout);
    let mut answered = 0;
    loop {
        let left = MAX_ANSWER.saturating_sub(answered) as u64 + 1;
        let mut bytes = Vec::new();
        match (&mut reader).take(left).read_until(b'\n', &mut bytes) {
            Ok(0) | Err(_) => return,
            Ok(read) => answered += read,
        }
        if answered > MAX_ANSWER {
            let _ = tx.send(Answer::TooLarge);
            return;
        }
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
        }
        let Ok(line) = String::from_utf8(bytes) else {
            return;
        };
        if line.ends_with(delimiter) {
            answered = 0;
        }
        if tx.send(Answer::Line(line)).is_err() {
            return;
        }
    }
}

fn engine_error(message: &str) -> RenderError {
    RenderError::Engine {
        engine: Engine::JavaPlantUml,
        message: message.into(),
    }
}

/// Creates `path` as a folder only its owner may enter.
#[cfg(unix)]
fn private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(path)
}

/// Creates `path` as a folder. Windows has no mode bits; the temporary
/// folder is the user's own already.
#[cfg(not(unix))]
fn private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new().create(path)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Windows has no execute bit: a file that is there runs.
#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file())
}

/// The first `@start…`/`@end…` block of `source` with a trailing newline,
/// and the zero-based source line of its first line (-1 when wrapped).
fn first_block(source: &str) -> (String, isize) {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_start().to_ascii_lowercase().starts_with("@start"));
    let Some(start) = start else {
        return (format!("@startuml\n{}\n@enduml\n", source.trim_end()), -1);
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.trim_start().to_ascii_lowercase().starts_with("@end"))
        .map_or(lines.len(), |i| start + 1 + i + 1);
    let mut block = lines[start..end].join("\n");
    block.push('\n');
    (block, start as isize)
}

/// Splits PlantUML's answer into an error (`ERROR`, line, message) and the SVG.
fn parse_output(lines: Vec<String>) -> (Option<(Option<usize>, String)>, String) {
    if lines.first().map(String::as_str) != Some("ERROR") {
        return (None, lines.join("\n"));
    }
    let line = lines.get(1).and_then(|l| l.trim().parse().ok());
    let svg_start = lines
        .iter()
        .skip(2)
        .position(|l| l.trim_start().starts_with('<'))
        .map_or(lines.len(), |i| i + 2);
    let message = lines[2.min(lines.len())..svg_start]
        .join(" ")
        .trim()
        .to_owned();
    let message = if message.is_empty() {
        "Syntax error".to_owned()
    } else {
        message
    };
    (Some((line, message)), lines[svg_start..].join("\n"))
}

/// What PlantUML gets of Omaspec's environment: where programs, Java,
/// Graphviz, fonts and temporary files are, the language, and PlantUML's size limit. Tokens,
/// keys and session addresses stay behind, whatever a diagram asks for.
/// `JAVA_TOOL_OPTIONS` is set on its own, ending with the profile;
/// `_JAVA_OPTIONS` and `JDK_JAVA_OPTIONS` would come after it.
const KEPT_ENV: [&str; 16] = [
    "PATH",
    "HOME",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "JAVA_HOME",
    "GRAPHVIZ_DOT",
    "PLANTUML_LIMIT_SIZE",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "XDG_CONFIG_DIRS",
    "XDG_DATA_DIRS",
    "FONTCONFIG_FILE",
    "FONTCONFIG_PATH",
];

/// `command` with an empty environment but for [`KEPT_ENV`].
fn kept_env(mut command: Command) -> Command {
    command.env_clear();
    for name in KEPT_ENV {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

/// `JAVA_TOOL_OPTIONS` for the PlantUML process: the sandbox's flags
/// (the scratch folder, the open-file limit) when `tmpdir` is set, then
/// the inherited value, then the sandbox profile.
fn java_tool_options(inherited: Option<&str>, tmpdir: Option<&Path>) -> String {
    let mut options = String::new();
    if let Some(dir) = tmpdir {
        options.push_str(&format!(
            "-XX:-UsePerfData -XX:-MaxFDLimit -Djava.io.tmpdir={}",
            dir.display()
        ));
    }
    if let Some(own) = inherited.filter(|own| !own.is_empty()) {
        if !options.is_empty() {
            options.push(' ');
        }
        options.push_str(own);
    }
    if !options.is_empty() {
        options.push(' ');
    }
    options.push_str("-DPLANTUML_SECURITY_PROFILE=SANDBOX");
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_first_block() {
        let (block, first) =
            first_block("' note\n\n@startuml\nA -> B\n@enduml\n@startuml\nC -> D\n@enduml");
        assert_eq!(block, "@startuml\nA -> B\n@enduml\n");
        assert_eq!(first, 2);
        let (block, first) = first_block("A -> B\n");
        assert_eq!(block, "@startuml\nA -> B\n@enduml\n");
        assert_eq!(first, -1);
    }

    #[test]
    fn parses_errors() {
        let out = |s: &str| s.lines().map(str::to_owned).collect::<Vec<_>>();
        let (err, svg) = parse_output(out("ERROR\n3\nSyntax Error?\n<svg>x</svg>"));
        assert_eq!(err, Some((Some(3), "Syntax Error?".into())));
        assert_eq!(svg, "<svg>x</svg>");
        let (err, svg) = parse_output(out("<svg>y</svg>"));
        assert_eq!(err, None);
        assert_eq!(svg, "<svg>y</svg>");
    }

    #[test]
    fn sandbox_profile_follows_inherited_java_tool_options() {
        let dir = Path::new("/tmp/omaspec-plantuml");
        let options = java_tool_options(
            Some("-DPLANTUML_SECURITY_PROFILE=UNSECURE -Xmx64m"),
            Some(dir),
        );
        let unsecure = options
            .find("-DPLANTUML_SECURITY_PROFILE=UNSECURE")
            .expect("inherited profile");
        let sandbox = options
            .rfind("-DPLANTUML_SECURITY_PROFILE=SANDBOX")
            .expect("sandbox profile");
        assert!(unsecure < sandbox, "{options}");
        assert!(options.contains("-XX:-UsePerfData"), "{options}");
        assert!(options.contains("-XX:-MaxFDLimit"), "{options}");
        assert!(
            options.contains(&format!("-Djava.io.tmpdir={}", dir.display())),
            "{options}"
        );
        assert_eq!(
            java_tool_options(None, None),
            "-DPLANTUML_SECURITY_PROFILE=SANDBOX"
        );
    }
}
