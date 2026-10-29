//! The examples that come with Omaspec: the editor's gallery
//! (Ctrl+Shift+E) shows them as tiles, `omaspec example` prints them.
//! Their source is `examples/gallery`, where CI lints and draws them.

use std::borrow::Cow;
use std::path::Path;
use std::sync::LazyLock;

use omaspec_core::Language;

/// What an example is about, for the gallery's filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    /// State machines, scenarios, activities, flows, timing, signal flow.
    Behavior,
    /// Classes, deployments, use cases, entities.
    Structure,
    /// Mind maps, work breakdowns, schedules.
    Plans,
    /// JSON and charts.
    Data,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Behavior, Group::Structure, Group::Plans, Group::Data];

    pub fn name(self) -> &'static str {
        match self {
            Group::Behavior => "behavior",
            Group::Structure => "structure",
            Group::Plans => "plans",
            Group::Data => "data",
        }
    }
}

pub struct Example {
    /// The file name it is saved under, which the tab takes.
    pub file: &'static str,
    pub title: &'static str,
    /// What it shows, in a few words.
    pub about: &'static str,
    pub group: Group,
    /// A larger one, that shows what the engines do with much at once.
    pub complex: bool,
    pub text: &'static str,
}

impl Example {
    pub fn language(&self) -> Language {
        Language::from_file_name(self.file).unwrap_or(Language::PlantUml)
    }

    /// The file name without its extension.
    pub fn stem(&self) -> &'static str {
        Path::new(self.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(self.file)
    }

    /// Whether every word of `query` is in its title, file name, kind,
    /// group, language or description, or is `complex` for a complex one,
    /// ignoring case.
    pub fn matches(&self, query: &str) -> bool {
        let hay = match EXAMPLES.iter().position(|e| e.file == self.file) {
            Some(at) => Cow::Borrowed(HAYSTACKS[at].as_str()),
            None => Cow::Owned(self.haystack()),
        };
        query
            .to_lowercase()
            .split_whitespace()
            .all(|word| hay.contains(word))
    }
}

impl Example {
    /// The words a search looks in, in lower case.
    fn haystack(&self) -> String {
        let kind = omaspec_core::classify(self.text, self.language());
        format!(
            "{} {} {} {} {} {} {}",
            self.title,
            self.file,
            crate::icons::label(&kind),
            self.group.name(),
            self.language().name(),
            self.about,
            if self.complex { "complex" } else { "" }
        )
        .to_lowercase()
    }
}

/// Each example's [`Example::haystack`], made once: the gallery searches
/// them every frame.
static HAYSTACKS: LazyLock<Vec<String>> =
    LazyLock::new(|| EXAMPLES.iter().map(Example::haystack).collect());

macro_rules! example {
    ($file:literal, $group:ident, $title:literal, $about:literal) => {
        example!($file, $group, $title, $about, false)
    };
    ($file:literal, $group:ident, $title:literal, $about:literal, $complex:literal) => {
        Example {
            file: $file,
            title: $title,
            about: $about,
            group: Group::$group,
            complex: $complex,
            text: include_str!(concat!("../../../examples/gallery/", $file)),
        }
    };
}

/// Every example, in the gallery's order: by group, the complex ones
/// after the others.
pub static EXAMPLES: &[Example] = &[
    example!(
        "coffee-machine.puml",
        Behavior,
        "A coffee machine",
        "Nested states, guards and a refill."
    ),
    example!(
        "crossing.mmd",
        Behavior,
        "A pedestrian crossing",
        "A Mermaid state machine with a nested state."
    ),
    example!(
        "pizza.puml",
        Behavior,
        "Pizza night",
        "A scenario with a loop and two endings."
    ),
    example!(
        "pull-request.mmd",
        Behavior,
        "A pull request",
        "A Mermaid sequence: review until approved."
    ),
    example!(
        "first-day.puml",
        Behavior,
        "A first day at work",
        "Swimlanes, a fork and a decision."
    ),
    example!(
        "friday.mmd",
        Behavior,
        "Deploy on a Friday?",
        "A Mermaid flowchart that decides."
    ),
    example!(
        "washing-machine.puml",
        Behavior,
        "A washing machine",
        "Timing: a program, a lock, the drum's speed."
    ),
    example!(
        "thermostat.puml",
        Behavior,
        "A room thermostat",
        "Signal flow between blocks and their ports."
    ),
    example!(
        "elevator.puml",
        Behavior,
        "An elevator",
        "Regions, a choice, an emergency and its way out.",
        true
    ),
    example!(
        "media-player.mmd",
        Behavior,
        "A media player",
        "Mermaid: nested states beside a second region.",
        true
    ),
    example!(
        "checkout.puml",
        Behavior,
        "Checking out",
        "Boxes, alt, loop with break, par, opt, numbering.",
        true
    ),
    example!(
        "incident.puml",
        Behavior,
        "Handling an incident",
        "Three lanes, branches, a fork and a retry loop.",
        true
    ),
    example!(
        "pipeline.mmd",
        Behavior,
        "A delivery pipeline",
        "A Mermaid flowchart in three subgraphs.",
        true
    ),
    example!(
        "bookshop.puml",
        Structure,
        "A bookshop",
        "Classes, an interface, an enum, multiplicities."
    ),
    example!(
        "smart-home.puml",
        Structure,
        "A smart home",
        "Devices, a hub and what talks to what."
    ),
    example!(
        "ticket-machine.puml",
        Structure,
        "A ticket machine",
        "Use cases, two actors and what they include."
    ),
    example!(
        "recipes.mmd",
        Structure,
        "Recipes",
        "A Mermaid entity relationship diagram."
    ),
    example!(
        "library.puml",
        Structure,
        "A library system",
        "Three packages, inheritance, composition, roles.",
        true
    ),
    example!(
        "garden.puml",
        Plans,
        "A garden year",
        "A mind map, branching both ways."
    ),
    example!(
        "moving-house.puml",
        Plans,
        "Moving house",
        "A work breakdown in three phases."
    ),
    example!(
        "treehouse.puml",
        Plans,
        "Building a treehouse",
        "A Gantt chart: tasks, order, a milestone."
    ),
    example!(
        "app-launch.puml",
        Plans,
        "Launching an app",
        "Three phases, overlaps, progress, milestones.",
        true
    ),
    example!(
        "settings.puml",
        Data,
        "Editor settings",
        "JSON drawn as nested tables."
    ),
    example!("day.mmd", Data, "Where a day goes", "A Mermaid pie chart."),
];

/// The example named `name`, with or without its extension.
pub fn find(name: &str) -> Option<&'static Example> {
    EXAMPLES.iter().find(|e| e.file == name || e.stem() == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_is_found_by_its_name_and_names_are_unique() {
        let mut stems: Vec<&str> = EXAMPLES.iter().map(Example::stem).collect();
        stems.sort_unstable();
        stems.dedup();
        assert_eq!(stems.len(), EXAMPLES.len());
        for example in EXAMPLES {
            assert_eq!(find(example.stem()).unwrap().file, example.file);
            assert_eq!(find(example.file).unwrap().file, example.file);
            assert_eq!(
                omaspec_core::detect(example.text, None),
                example.language(),
                "{}",
                example.file
            );
        }
        assert!(find("nothing").is_none());
    }

    #[test]
    fn every_word_of_a_query_has_to_match() {
        let coffee = find("coffee-machine").unwrap();
        assert!(coffee.matches("Coffee"));
        assert!(!coffee.matches("complex"));
        assert!(find("elevator").unwrap().matches("complex state"));
        assert!(find("pizza").unwrap().matches("sequence"));
        assert!(coffee.matches("plantuml behavior"));
        assert!(!coffee.matches("coffee mermaid"));
        assert!(coffee.matches(""));
    }
}
