//! The same picture in every system language: each example drawn under
//! several locales, the SVGs compared byte for byte. Mermaid's swimlane
//! layout ranks its nodes by the browser's language (mermaid-js/mermaid
//! #8759); Omaspec sorts and measures with no locale, and this holds it
//! to that. The Java PlantUML is left out: what it draws is its own.
//!
//! The environment alone decides nothing where a locale is not installed,
//! so CI installs them and sets `OMASPEC_REQUIRE_LOCALES=1`: then a
//! missing one fails the test instead of passing unseen.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `C` to compare with, then languages that sort, fold case and write
/// numbers their own way: German, Turkish (`i` and `İ`), Japanese, Arabic.
const LOCALES: [&str; 5] = [
    "C",
    "de_DE.UTF-8",
    "tr_TR.UTF-8",
    "ja_JP.UTF-8",
    "ar_EG.UTF-8",
];

/// Names that a locale would put in another order or another case.
const SORTED: [(&str, &str); 3] = [
    (
        "names-flow.mmd",
        "flowchart TD\n  Zebra --> apple\n  Zebra --> Ärger\n  Zebra --> İstanbul\n  Zebra --> ılık\n  Zebra --> item\n  Zebra --> Item\n  apple --> Öl\n  Ärger --> Öl\n  item --> Öl\n",
    ),
    (
        "names-states.puml",
        "@startuml\n[*] --> Zebra\nZebra --> apple : I\nZebra --> Ärger : i\nZebra --> İstanbul : ı\nZebra --> item : 1,5\napple --> [*]\n@enduml\n",
    ),
    (
        "names-classes.puml",
        "@startuml\nclass Zebra\nclass apple\nclass Ärger\nclass İstanbul\nclass item\nZebra --> apple\nZebra --> Ärger\nZebra --> İstanbul\nZebra --> item\n@enduml\n",
    ),
];

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if name.ends_with(".puml") || name.ends_with(".mmd") {
            fs::copy(&path, to.join(name)).unwrap();
        }
    }
}

/// Every example and the names above, drawn under `locale`: each SVG by
/// its file name.
fn drawn(locale: &str) -> Vec<(String, Vec<u8>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "omaspec-locale-{}-{}",
        locale.replace('.', "-"),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    // Each in a folder of its own: `library.puml` and `library.mmd` would
    // write the same picture.
    for file in fs::read_dir(&root).unwrap() {
        let path = file.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if name.ends_with(".puml") || name.ends_with(".mmd") {
            let folder = dir.join(name.replace('.', "-"));
            fs::create_dir_all(&folder).unwrap();
            fs::copy(&path, folder.join(name)).unwrap();
        }
    }
    copy(&root.join("gallery"), &dir.join("gallery"));
    for (name, text) in SORTED {
        fs::write(dir.join(name), text).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(["render", dir.to_str().unwrap()])
        .env("LANG", locale)
        .env("LC_ALL", locale)
        .env("LANGUAGE", locale.split(['_', '.']).next().unwrap())
        .env("OMASPEC_PLANTUML", "none")
        .output()
        .expect("omaspec runs");
    assert!(
        out.status.success(),
        "{locale}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut svgs = Vec::new();
    let mut folders = vec![dir.clone()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "svg") {
                let name = path
                    .strip_prefix(&dir)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                svgs.push((name, fs::read(&path).unwrap()));
            }
        }
    }
    svgs.sort();
    let _ = fs::remove_dir_all(&dir);
    svgs
}

/// The locales of `LOCALES` this system has not installed.
fn missing() -> Vec<&'static str> {
    let listed = Command::new("locale")
        .arg("-a")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_lowercase())
        .unwrap_or_default();
    let installed: Vec<String> = listed.lines().map(|l| l.replace('-', "")).collect();
    LOCALES
        .into_iter()
        .filter(|locale| !installed.contains(&locale.to_lowercase().replace('-', "")))
        .collect()
}

#[test]
fn every_example_is_drawn_the_same_in_every_system_language() {
    let missing = missing();
    if std::env::var_os("OMASPEC_REQUIRE_LOCALES").is_some() {
        assert!(missing.is_empty(), "locales not installed: {missing:?}");
    } else if !missing.is_empty() {
        eprintln!("not installed, so set by name only: {missing:?}");
    }
    let plain = drawn(LOCALES[0]);
    assert!(
        plain.len() >= 41 && plain.iter().all(|(_, svg)| svg.starts_with(b"<svg")),
        "{} pictures",
        plain.len()
    );
    for locale in &LOCALES[1..] {
        let other = drawn(locale);
        let names = |svgs: &[(String, Vec<u8>)]| -> Vec<String> {
            svgs.iter().map(|(name, _)| name.clone()).collect()
        };
        assert_eq!(names(&plain), names(&other), "{locale}");
        for ((name, a), (_, b)) in plain.iter().zip(&other) {
            assert!(a == b, "{name} is drawn differently under {locale}");
        }
    }
}
