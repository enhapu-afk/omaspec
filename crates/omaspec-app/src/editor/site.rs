//! The project as one page (`omaspec export DIR --html`,
//! [`omaspec_render::export::project`]): its map, and every diagram and
//! folder map the map leads to, each drawn once in the export's look,
//! with where a click in it goes. The way is found as the window dives:
//! from the map along the tiles' links, a folder's tile into its map, a
//! linked state into its diagram; a link that names several diagrams
//! goes nowhere, as in the window. A diagram that does not draw is told
//! and left out, and the links to it go nowhere.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use omaspec_render::export::flight::Picture;
use omaspec_render::export::project::{Hop, Site, page};
use omaspec_render::{Link, Renderer};

use super::timelapse::Units;
use crate::look::Look;
use crate::specs::{Maps, read_project};

/// What a picture is: the map of a folder, or a diagram by its path in
/// the project.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Is {
    Map(String),
    Diagram(String),
}

/// A picture as drawn: its SVG, its size and its links, both in its own
/// units from its corner.
type Drawn = (String, (f32, f32), Vec<Link>);

/// What [`write`] wrote, and what it left out.
#[derive(Debug)]
pub(crate) struct Written {
    pub page: PathBuf,
    pub pictures: usize,
    /// The diagrams that did not draw: their paths in the project, and why.
    pub failed: Vec<(String, String)>,
}

/// Writes the page of the project at `root` into `out` (its `export`
/// folder where none is named), in `look` or the one the project pins, the
/// containers of `folds` folded in every diagram that holds them; a name
/// none of its diagrams holds writes nothing.
pub(crate) fn write(
    root: &Path,
    out: Option<&Path>,
    look: Option<Look>,
    folds: &[String],
    renderer: &mut Renderer,
) -> Result<Written> {
    let root =
        std::path::absolute(root).with_context(|| format!("no folder `{}`", root.display()))?;
    anyhow::ensure!(root.is_dir(), "{} is not a folder", root.display());
    let (_, atlas) = read_project(&root);
    anyhow::ensure!(
        !atlas.diagrams.is_empty(),
        "{} holds no diagrams",
        root.display()
    );
    let name = crate::project_export::export_name(atlas.name.as_deref(), &root);
    // What the page and its trail call the project: its name as written.
    let title = atlas
        .name
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map_or_else(|| name.clone(), str::to_owned);
    let skin = crate::look::skin_for(look, Some(&root))?;
    let atlas = Arc::new(atlas);
    let mut maps = Maps::with(root.clone(), Arc::clone(&atlas));
    // The containers of `folds` some diagram held, and every one they have.
    let (mut held, mut offered): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let mut draw = |is: &Is| -> Result<Drawn, String> {
        let rendered = match is {
            Is::Map(folder) => {
                crate::look::render_map(&mut maps, renderer, &root, folder, skin.as_ref())
            }
            Is::Diagram(file) => {
                let entry = atlas
                    .diagrams
                    .iter()
                    .find(|e| e.file == *file)
                    .ok_or_else(|| format!("{file} is not in the project"))?;
                crate::folds::render(
                    &mut maps,
                    renderer,
                    &entry.source,
                    entry.language,
                    Some(&root.join(&entry.file)),
                    entry.place.as_ref(),
                    skin.as_ref(),
                    folds,
                )
                .map(|(rendered, folded)| {
                    folded.gather(&mut held, &mut offered);
                    rendered
                })
            }
        }
        .map_err(|err| err.to_string())?;
        let size = omaspec_render::export::parse(&rendered.svg)
            .map_err(|err| err.to_string())?
            .size();
        // The links and the size in the picture's own units, from its
        // corner, which the page draws in.
        let units = Units::of(&rendered.svg, (size.width(), size.height()));
        let (cx, cy) = units.point((0.0, 0.0));
        let links = rendered
            .links
            .iter()
            .map(|link| {
                let mut link = units.link(link);
                link.rect.x -= cx;
                link.rect.y -= cy;
                if let Some(room) = &mut link.room {
                    room.x -= cx;
                    room.y -= cy;
                }
                link
            })
            .collect();
        Ok((
            rendered.svg,
            units.size((size.width(), size.height())),
            links,
        ))
    };
    let top = Is::Map(String::new());
    let (svg, size, links) = draw(&top)
        .map_err(|err| anyhow::anyhow!("the map of {} did not draw: {err}", root.display()))?;
    let mut pictures = vec![Picture {
        svg,
        size,
        name: title.clone(),
    }];
    let mut links_of: Vec<Vec<Link>> = vec![links];
    // For each picture, the folder its links are paths in.
    let mut bases: Vec<String> = vec![String::new()];
    // Each picture by what it is; none for a diagram that did not draw.
    let mut found: HashMap<Is, Option<usize>> = HashMap::from([(top, Some(0))]);
    let mut failed = Vec::new();
    let mut hops: Vec<Vec<Hop>> = Vec::new();
    let mut at = 0;
    while at < pictures.len() {
        let mut from_here = Vec::new();
        let base = bases[at].clone();
        for link in std::mem::take(&mut links_of[at]) {
            let is = match (&link.folder, link.files.as_slice()) {
                (Some(folder), _) => Is::Map(within(&base, folder)),
                (None, [one]) => Is::Diagram(within(&base, one)),
                // Several of that name, or none: the window goes nowhere.
                _ => continue,
            };
            let to = match found.get(&is) {
                Some(&known) => known,
                None => {
                    let drawn = draw(&is);
                    let index = match drawn {
                        Ok((svg, size, links)) => {
                            pictures.push(Picture {
                                svg,
                                size,
                                name: link.name.clone(),
                            });
                            links_of.push(links);
                            bases.push(base_of(&atlas, &root, &is));
                            Some(pictures.len() - 1)
                        }
                        Err(why) => {
                            failed.push(match &is {
                                Is::Diagram(file) => (file.clone(), why),
                                Is::Map(folder) => {
                                    (folder.clone(), format!("its map did not draw: {why}"))
                                }
                            });
                            None
                        }
                    };
                    found.insert(is, index);
                    index
                }
            };
            let Some(to) = to else {
                continue;
            };
            let rect = [link.rect.x, link.rect.y, link.rect.w, link.rect.h];
            let room = link.room.map_or(rect, |r| [r.x, r.y, r.w, r.h]);
            from_here.push(Hop {
                rect,
                room,
                to,
                name: link.name,
            });
        }
        hops.push(from_here);
        at += 1;
    }
    // A diagram that did not draw may hold it: what did not draw is said
    // instead.
    if failed.is_empty()
        && let Some(name) = crate::folds::first_unheld(folds, &held)
    {
        anyhow::bail!(crate::folds::held_by_none(name, &offered));
    }
    let paper = super::building::picture::paper_of(&pictures[0].svg);
    let paper = omaspec_theme::Rgb::parse_hex(&paper).unwrap_or(omaspec_theme::Rgb {
        r: 255,
        g: 255,
        b: 255,
    });
    let dark = (u32::from(paper.r) * 3 + u32::from(paper.g) * 6 + u32::from(paper.b)) < 1280;
    let palette = if dark {
        omaspec_core::skin::DARK
    } else {
        omaspec_core::skin::LIGHT
    };
    let color = |hex: &str| {
        omaspec_theme::Rgb::parse_hex(hex).map_or(omaspec_uml::Color::BLACK, |c| {
            omaspec_uml::Color(c.r, c.g, c.b)
        })
    };
    // In the look auto every picture turns dark under a dark scheme, and
    // the page with it.
    let auto = skin.as_ref().is_some_and(|skin| skin.is_auto());
    let dark_too = auto.then(|| {
        let d = omaspec_core::skin::DARK;
        (color(d.background), color(d.text), color(d.accent))
    });
    let site = Site {
        paper: omaspec_uml::Color(paper.r, paper.g, paper.b),
        ink: color(palette.text),
        accent: color(palette.accent),
        dark: dark_too,
        pictures,
        hops,
    };
    let written = page(&site, &title).map_err(|err| anyhow::anyhow!("{err}"))?;
    // `-o` names the page where it ends in `.html`, else its folder.
    let named = out.filter(|out| {
        out.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
    });
    let path = match (named, out) {
        (Some(page), _) => {
            std::path::absolute(page).with_context(|| format!("no file `{}`", page.display()))?
        }
        (None, Some(out)) => std::path::absolute(out)
            .with_context(|| format!("no folder `{}`", out.display()))?
            .join(format!("{name}.html")),
        (None, None) => root
            .join(crate::project_export::FOLDER)
            .join(format!("{name}.html")),
    };
    let folder = path.parent().context("a page lies in a folder")?;
    anyhow::ensure!(!folder.is_file(), "{} is a file", folder.display());
    std::fs::create_dir_all(folder).with_context(|| format!("cannot make {}", folder.display()))?;
    crate::files::write_atomic(&path, written.as_bytes())
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(Written {
        page: path,
        pictures: site.pictures.len(),
        failed,
    })
}

/// `path`, a path in the folder `base` of the project, as a path in the
/// project.
fn within(base: &str, path: &str) -> String {
    if base.is_empty() {
        path.to_owned()
    } else {
        format!("{base}/{path}")
    }
}

/// The folder, in the project at `root`, whose paths the links in the
/// picture `is` name: a diagram of a project inside this one links among
/// that project's diagrams, as it is drawn ([`crate::specs::Maps`]); a
/// map, made from this project, among its own.
fn base_of(atlas: &omaspec_render::Atlas, root: &Path, is: &Is) -> String {
    let Is::Diagram(file) = is else {
        return String::new();
    };
    atlas
        .diagrams
        .iter()
        .find(|e| e.file == *file)
        .and_then(|e| e.place.as_ref())
        .and_then(|place| crate::specs::path_in(root, &place.root))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The map, a folder's map and the diagrams they lead to, each once,
    /// with where a click goes; a diagram that does not draw is told and
    /// its tile goes nowhere.
    #[test]
    fn a_project_is_one_page_from_its_map_down() {
        let work = std::env::temp_dir().join(format!("omaspec-site-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(work.join("kitchen")).unwrap();
        let write = |file: &str, text: &str| std::fs::write(work.join(file), text).unwrap();
        write("omaspec-project.toml", "name = \"cafe\"\n");
        write(
            "day.puml",
            "@startuml\ntitle Day\n[*] --> Closed\nClosed --> Order : guest\n@enduml\n",
        );
        write(
            "order.puml",
            "@startuml\ntitle Order\n[*] --> Taken\nTaken --> [*] : paid\n@enduml\n",
        );
        write(
            "kitchen/oven.puml",
            "@startuml\ntitle Oven\n[*] --> Cold\nCold --> Hot : heat\n@enduml\n",
        );
        // Omaspec's own syntax, refused: no other engine draws it either.
        write(
            "plan.puml",
            "@startstorymap\n* a story without an activity\n@endstorymap\n",
        );
        let mut renderer = Renderer::new(&crate::commands::render_config());
        let written = write_site(&work, &mut renderer);
        let page = std::fs::read_to_string(&written.page).unwrap();
        assert!(
            written.page.ends_with("export/cafe.html"),
            "{:?}",
            written.page
        );
        assert!(page.contains(r#""kind":"project""#));
        // The map, Day, Order and the oven: a folder of one diagram is
        // that diagram's tile on the map.
        assert_eq!(written.pictures, 4);
        assert_eq!(
            page.matches(r#"<div class="picture""#).count(),
            written.pictures
        );
        // Day's state Order opens the order: a hop in Day's picture.
        assert!(page.contains(r#""name":"Order""#), "{page}");
        assert!(page.contains(r#""name":"Oven""#), "{page}");
        assert!(
            written.failed.iter().any(|(file, _)| file == "plan.puml"),
            "{:?}",
            written.failed
        );
        let _ = std::fs::remove_dir_all(&work);
    }

    /// A project inside the project links among its own diagrams: a
    /// state there opens the diagram beside it, not one of that path at
    /// the top.
    #[test]
    fn a_project_inside_links_among_its_own_diagrams() {
        let work = std::env::temp_dir().join(format!("omaspec-site-inner-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(work.join("inner")).unwrap();
        let write = |file: &str, text: &str| std::fs::write(work.join(file), text).unwrap();
        write("omaspec-project.toml", "name = \"Cafe: day/night\"\n");
        write(
            "day.puml",
            "@startuml\ntitle Day\n[*] --> Closed\nClosed --> Open : guest\n@enduml\n",
        );
        write("inner/omaspec-project.toml", "name = \"inner\"\n");
        write(
            "inner/kitchen.puml",
            "@startuml\ntitle Kitchen\n[*] --> Idle\nIdle --> Shift : open\n@enduml\n",
        );
        write(
            "inner/shift.puml",
            "@startuml\ntitle Shift\n[*] --> Cooking\nCooking --> [*] : done\n@enduml\n",
        );
        let mut renderer = Renderer::new(&crate::commands::render_config());
        let written = write_site(&work, &mut renderer);
        assert!(written.failed.is_empty(), "{:?}", written.failed);
        let page = std::fs::read_to_string(&written.page).unwrap();
        // The kitchen's state Shift opens the shift beside it, drawn once.
        assert!(page.contains(r#""name":"Shift""#), "{page}");
        assert_eq!(
            page.matches(r#"<div class="picture""#).count(),
            written.pictures
        );
        // The page and its trail say the project's name as written; its
        // file holds what a file's name may.
        assert!(page.contains("<title>Cafe: day/night</title>"), "{page}");
        assert!(
            written.page.ends_with("export/Cafe- day-night.html"),
            "{:?}",
            written.page
        );
        // `-o` ending in `.html` names the page itself; another, its folder.
        let named = work.join("out/site.html");
        let written = super::write(&work, Some(&named), None, &[], &mut renderer).unwrap();
        assert_eq!(written.page, named);
        assert!(named.is_file());
        let folder = work.join("pages");
        let written = super::write(&work, Some(&folder), None, &[], &mut renderer).unwrap();
        assert_eq!(written.page, folder.join("Cafe- day-night.html"));
        let _ = std::fs::remove_dir_all(&work);
    }

    fn write_site(work: &Path, renderer: &mut Renderer) -> Written {
        write(work, None, None, &[], renderer).unwrap()
    }
}
