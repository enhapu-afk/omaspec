//! What a long play may cost (REQ-074): a GIF of some 450 frames, drawn
//! without a GPU as a tester's machine and a CI runner draw it, within a
//! budget of memory and of time. A play that kept every frame's buffers
//! grew by megabytes a frame and passed every other test.
//!
//! Run as a release build (`cargo test --release -p omaspec-app --test
//! budget`, CI too): a debug build draws ten times slower.

#![cfg(target_os = "linux")]

use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The most memory the play may hold at once, in MiB. It peaks near 150
/// here; before the frames lay in one buffer it passed 1,000.
const MEMORY: u64 = 400;
/// The longest it may take: some 40 s here, and a runner is slower.
const TIME: Duration = Duration::from_secs(300);

/// The most memory the process `pid` has held so far, in MiB, as the
/// kernel tells it (`VmHWM`); `None` once it is gone.
fn peak(pid: u32) -> Option<u64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

#[test]
fn a_long_play_without_a_gpu_stays_within_its_budget() {
    if cfg!(debug_assertions) {
        return;
    }
    let dir = std::env::temp_dir().join(format!("omaspec-budget-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    // A machine of two regions and thirty steps, as a mission's flight is.
    let mut source = String::from("@startuml\n[*] --> Flight\nstate Flight {\n  [*] --> A0\n");
    let mut events = Vec::new();
    for i in 0..15 {
        source.push_str(&format!("  A{i} --> A{} : a{i}\n", i + 1));
        events.push(format!("a{i}"));
    }
    source.push_str("  ||\n  [*] --> B0\n");
    for i in 0..15 {
        source.push_str(&format!("  B{i} --> B{} : b{i}\n", i + 1));
        events.push(format!("b{i}"));
    }
    source.push_str("}\n@enduml\n");
    let file = dir.join("flight.puml");
    fs::write(&file, source).unwrap();
    let gif = dir.join("flight.gif");
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(["play", file.to_str().unwrap(), "-o", gif.to_str().unwrap()])
        .args(["--width", "1400", "--fps", "20", "--steps", "30"])
        .args(["--theme", "built-in", "--whole"])
        .args(["--events", &events.join(",")])
        .env("ICED_TEST_BACKEND", "tiny-skia")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("omaspec runs");
    let mut most = 0;
    let status = loop {
        if let Some(now) = peak(child.id()) {
            most = most.max(now);
        }
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < TIME,
            "the play takes over {TIME:?}; it held {most} MiB so far"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let said = child
        .wait_with_output()
        .map(|out| String::from_utf8_lossy(&out.stderr).into_owned())
        .unwrap_or_default();
    assert!(status.success(), "{said}");
    // Enough frames for a leak by the frame to show.
    let frames: usize = said
        .split('(')
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    assert!(frames >= 300, "{frames} frames: {said}");
    assert!(
        most <= MEMORY,
        "{frames} frames held {most} MiB, over the budget of {MEMORY}"
    );
    eprintln!(
        "{frames} frames, {most} MiB at most, {:.0} s",
        started.elapsed().as_secs_f32()
    );
    let _ = fs::remove_dir_all(&dir);
}
