//! A dive and a flight through a project as a page in vectors
//! ([`omaspec_render::export::flight`]): the map and each diagram on the
//! way drawn as the window draws them, in the theme's colors with their
//! links, and for each step the room the next picture grows out of: a
//! tile's picture, a state's box. The way is found as the window's flight
//! finds it (`--dive Day/Order`, [`super::moving`]); nothing is drawn in
//! pixels, so it needs no GPU.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use omaspec_core::Language;
use omaspec_render::export::flight as page;
use omaspec_render::{Atlas, Link, Place, Renderer};

use super::moving::Moving;
use super::{App, Message, Start};

/// A picture on the way, and the links in it.
struct Shown {
    svg: String,
    size: (f32, f32),
    name: String,
    links: Vec<Link>,
    /// Where its top left corner lies in its units, which a room on the
    /// page is measured from: not 0, 0 where its `viewBox` starts elsewhere.
    corner: (f32, f32),
}

/// What a picture is: the map of a folder, or a diagram by its path in
/// the project.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Is {
    Map(String),
    Diagram(String),
}

/// Writes the flights of `args.dives` through the project of `args.file`
/// as a page; what it wrote.
pub(super) fn write(args: &Moving) -> Result<String> {
    let file = std::fs::canonicalize(&args.file)
        .with_context(|| format!("cannot find {}", args.file.display()))?;
    let root = file
        .parent()
        .context("a project lies in a folder")?
        .to_owned();
    let start = Start {
        files: vec![file.clone()],
        folder: None,
        forced: args.forced,
    };
    let mut app = super::moving::film_app(args, start)?;
    let (specs, atlas) = crate::specs::read_project(&root);
    let _ = app.update(Message::Specs(
        root.clone(),
        Arc::new(specs),
        Arc::new(atlas),
    ));
    let key = app.doc().key;
    let (atlas, _) = app
        .linked_of(key)
        .context("the project could not be read")?;
    let project = app.project_name.clone().unwrap_or_else(|| "map".to_owned());
    let mut drawn: Vec<(Is, Shown)> = Vec::new();
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let mut flights = Vec::new();
    for name in &args.dives {
        let parts: Vec<&str> = name.split('/').collect();
        anyhow::ensure!(
            parts.iter().all(|part| !part.trim().is_empty()),
            "{name}: a flight names a tile and the links after it, as Orders/Checkout"
        );
        let mut here = picture(
            &mut app,
            &mut renderer,
            &atlas,
            &root,
            Is::Map(String::new()),
            &project,
            &mut drawn,
        )?;
        let mut steps = Vec::new();
        let mut next = 0;
        let mut last: Option<String> = None;
        while next < parts.len() {
            let links = &drawn[here].1.links;
            // The longest run of the parts left that names a link here.
            let found = (next + 1..=parts.len()).rev().find_map(|end| {
                let wanted = parts[next..end].join("/");
                links
                    .iter()
                    .find(|one| one.name.trim() == wanted.trim())
                    .map(|one| (end, one.clone()))
            });
            let Some((end, link)) = found else {
                let what = match &last {
                    None => "no tile of the map".to_owned(),
                    Some(last) => format!("no link of {last}"),
                };
                let has = if links.is_empty() {
                    "none".to_owned()
                } else {
                    links
                        .iter()
                        .map(|one| one.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                bail!("{} is {what}; it has {has}", parts[next].trim());
            };
            let is = match (&link.folder, link.files.as_slice()) {
                (Some(folder), _) => Is::Map(folder.clone()),
                (None, [one]) => Is::Diagram(one.clone()),
                (None, []) => bail!("{} opens no diagram", link.name),
                (None, many) => bail!(
                    "{} opens no diagram: {} diagrams are called {}: {}; give one of them another title",
                    link.name,
                    many.len(),
                    link.name,
                    many.join(", ")
                ),
            };
            let (cx, cy) = drawn[here].1.corner;
            let to = picture(
                &mut app,
                &mut renderer,
                &atlas,
                &root,
                is,
                &link.name,
                &mut drawn,
            )?;
            let room = link.room.unwrap_or(link.rect);
            steps.push(page::Step {
                from: here,
                to,
                room: [room.x - cx, room.y - cy, room.w, room.h],
            });
            last = Some(link.name.clone());
            here = to;
            next = end;
        }
        flights.push(steps);
    }
    let color = |rgb: omaspec_theme::Rgb| omaspec_uml::Color(rgb.r, rgb.g, rgb.b);
    let flight = page::Flight {
        pictures: drawn
            .into_iter()
            .map(|(_, shown)| page::Picture {
                svg: shown.svg,
                size: shown.size,
                name: shown.name,
            })
            .collect(),
        flights,
        paper: color(app.palette.background),
        ink: color(app.palette.foreground),
    };
    // A map has no source of its own: it is made from the project, as
    // `omaspec render` draws it.
    let written = page::page(&flight, None, &project).map_err(|err| anyhow::anyhow!("{err}"))?;
    crate::files::write_atomic(&args.output, written.as_bytes())
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} (a page, {} KB, into {} and back in vectors)",
        args.file.display(),
        args.output.display(),
        written.len().div_ceil(1024),
        args.dives.join(", ")
    ))
}

/// The index of the picture `is` among those `drawn`, drawn now if it is
/// not yet, called `name`.
fn picture(
    app: &mut App,
    renderer: &mut Renderer,
    atlas: &Atlas,
    root: &Path,
    is: Is,
    name: &str,
    drawn: &mut Vec<(Is, Shown)>,
) -> Result<usize> {
    if let Some(at) = drawn.iter().position(|(one, _)| *one == is) {
        return Ok(at);
    }
    let rendered = match &is {
        Is::Map(folder) => {
            let (skin, _) = app.diagram_source(crate::look::MAP_SKIN, Language::PlantUml);
            renderer.render_map(atlas, folder, &skin)
        }
        Is::Diagram(own) => {
            let path: PathBuf = root.join(own);
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            let language = omaspec_core::detect(&text, Language::from_file_name(own));
            let (source, pending) = app.diagram_source_in(Some(root), &text, language);
            // Its folder, where an `!include` finds its file.
            let place = Place::of(&path, Some(root));
            renderer
                .render_linked(&source, language, place.as_ref(), Some(own), atlas)
                .map_err(|mut err| {
                    // The line of the file, not of the look laid over it.
                    if let Some(pending) = pending {
                        pending.shift.remap(&mut err);
                    }
                    err
                })
        }
    }
    .map_err(|err| anyhow::anyhow!("{name}: {err}"))?;
    let size = omaspec_render::export::parse(&rendered.svg)
        .map_err(|err| anyhow::anyhow!("{err}"))?
        .size();
    // The links, and the size, in the picture's own units, which the
    // page draws in.
    let units = super::timelapse::Units::of(&rendered.svg, (size.width(), size.height()));
    let links = rendered.links.iter().map(|link| units.link(link)).collect();
    drawn.push((
        is,
        Shown {
            size: units.size((size.width(), size.height())),
            svg: rendered.svg,
            name: name.to_owned(),
            links,
            corner: units.point((0.0, 0.0)),
        },
    ));
    Ok(drawn.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flight into a tile and on along its link, as a page: each
    /// picture once, a step for each link with the room it grows out of;
    /// a link not there is named with those there.
    #[test]
    fn a_flight_is_written_as_a_page() {
        let work = std::env::temp_dir().join(format!("omaspec-flight-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let write = |file: &str, text: &str| std::fs::write(work.join(file), text).unwrap();
        write("omaspec-project.toml", "name = \"cafe\"\n");
        // A diagram on the way finds what it includes in its folder.
        write("common.iuml", "skinparam shadowing false\n");
        write(
            "day.puml",
            "@startuml\ntitle Day\n!include common.iuml\n[*] --> Closed\nClosed --> Order : guest\n@enduml\n",
        );
        write(
            "order.puml",
            "@startuml\ntitle Order\n[*] --> Taken\nTaken --> [*] : paid\n@enduml\n",
        );
        let moving = |dives: &[&str], output: &str| Moving {
            file: work.join("omaspec-project.toml"),
            text: None,
            output: work.join(output),
            events: Vec::new(),
            steps: Some(100),
            width: None,
            fps: None,
            folds: Vec::new(),
            dives: dives.iter().map(|d| (*d).to_owned()).collect(),
            builds: false,
            bare: false,
            lockstep: false,
            history: false,
            travel: None,
            traffic: Vec::new(),
            themes: None,
            ink: false,
            look: None,
            timing: Default::default(),
            whole: false,
            theme: Some("built-in".into()),
            file_colors: false,
            root: None,
            forced: None,
            renderer: None,
            quality: super::super::gifs::Quality::default(),
        };
        let told = super::super::moving::play(&moving(&["Day/Order", "Order"], "f.html")).unwrap();
        assert!(
            told.contains("a page") && told.contains("Day/Order"),
            "{told}"
        );
        let page = std::fs::read_to_string(work.join("f.html")).unwrap();
        // The map, Day and Order, each once; two flights.
        assert_eq!(page.matches(r#"<div class="picture""#).count(), 3);
        assert!(page.contains(r#""kind":"flight""#));
        assert!(
            page.contains(r#""from":0,"room""#) && page.contains(r#""from":1"#),
            "{page}"
        );
        // A map has no source of its own.
        assert!(!page.contains("sf:source"), "{page}");
        let refused = super::super::moving::play(&moving(&["Day/Nowhere"], "n.html"))
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("Nowhere is no link of Day") && refused.contains("Order"),
            "{refused}"
        );
        let _ = std::fs::remove_dir_all(&work);
    }
}
