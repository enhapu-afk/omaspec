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
/// folder where none is named), in `look` or the one the project pins.
pub(crate) fn write(
    root: &Path,
    out: Option<&Path>,
    look: Option<Look>,
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
    let skin = crate::look::skin_for(look, Some(&root))?;
    let atlas = Arc::new(atlas);
    let mut maps = Maps::with(root.clone(), Arc::clone(&atlas));
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
                crate::look::render(
                    &mut maps,
                    renderer,
                    &entry.source,
                    entry.language,
                    Some(&root.join(&entry.file)),
                    entry.place.as_ref(),
                    skin.as_ref(),
                )
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
        name: name.clone(),
    }];
    let mut links_of: Vec<Vec<Link>> = vec![links];
    // Each picture by what it is; none for a diagram that did not draw.
    let mut found: HashMap<Is, Option<usize>> = HashMap::from([(top, Some(0))]);
    let mut failed = Vec::new();
    let mut hops: Vec<Vec<Hop>> = Vec::new();
    let mut at = 0;
    while at < pictures.len() {
        let mut from_here = Vec::new();
        for link in std::mem::take(&mut links_of[at]) {
            let is = match (&link.folder, link.files.as_slice()) {
                (Some(folder), _) => Is::Map(folder.clone()),
                (None, [one]) => Is::Diagram(one.clone()),
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
                            Some(pictures.len() - 1)
                        }
                        Err(why) => {
                            if let Is::Diagram(file) = &is {
                                failed.push((file.clone(), why));
                            }
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
    let written = page(&site, &name).map_err(|err| anyhow::anyhow!("{err}"))?;
    let folder = match out {
        Some(out) => {
            std::path::absolute(out).with_context(|| format!("no folder `{}`", out.display()))?
        }
        None => root.join(crate::project_export::FOLDER),
    };
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("cannot make {}", folder.display()))?;
    let path = folder.join(format!("{name}.html"));
    crate::files::write_atomic(&path, written.as_bytes())
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(Written {
        page: path,
        pictures: site.pictures.len(),
        failed,
    })
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

    fn write_site(work: &Path, renderer: &mut Renderer) -> Written {
        write(work, None, None, renderer).unwrap()
    }
}
