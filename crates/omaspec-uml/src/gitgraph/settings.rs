//! A git graph's settings: Mermaid's `gitGraph` section of its config.

/// What a git graph's settings ask for, Mermaid's `gitGraph` section of
/// its config, as the front matter or an init directive writes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// `mainBranchName`: the branch the graph starts on.
    pub main_name: String,
    /// `mainBranchOrder`: its place among the branches that ask for one.
    pub main_order: f32,
    /// `showBranches`: each lane's name and line.
    pub show_branches: bool,
    /// `showCommitLabel`: each commit's id.
    pub show_commit_label: bool,
    /// `rotateCommitLabel`: the ids turned by 45°, so that where time runs
    /// to the right the turns stand closer. Mermaid turns them unless told
    /// not to; Omaspec lays them level, the turns as far apart as they
    /// ask, unless told to turn them.
    pub rotate_commit_label: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            main_name: "main".to_owned(),
            main_order: 0.0,
            show_branches: true,
            show_commit_label: true,
            rotate_commit_label: false,
        }
    }
}

/// The settings of the `gitGraph` section that are carried out, as the
/// front matter writes them.
pub const SETTINGS: [&str; 5] = [
    "mainBranchName",
    "mainBranchOrder",
    "showBranches",
    "showCommitLabel",
    "rotateCommitLabel",
];

#[cfg(test)]
mod tests {
    use super::super::{Direction, draw, from_mermaid, from_mermaid_with, mermaid_refusal};
    use super::*;

    const RELEASE: &str = "gitGraph\n  commit id: \"initial-setup\"\n  branch feature\n  commit id: \"login-form\" tag: \"v0.9\"\n  checkout trunk\n  merge feature id: \"merge-feature\"\n";

    fn trunk() -> Settings {
        Settings {
            main_name: "trunk".to_owned(),
            ..Settings::default()
        }
    }

    /// `mainBranchName`: the graph starts on the branch so named, and a
    /// checkout of it is no checkout of a branch that is not there.
    #[test]
    fn the_main_branch_has_the_name_the_settings_give_it() {
        assert_eq!(
            mermaid_refusal(RELEASE, &Settings::default()).map(|(line, _)| line),
            Some(4)
        );
        let graph = from_mermaid_with(RELEASE, &trunk()).unwrap();
        let names: Vec<&str> = graph.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["trunk", "feature"]);
        assert!(draw(&graph).unwrap().contains(">trunk<"));
        // `main` is then a branch like any other.
        let graph =
            from_mermaid_with("gitGraph\n  commit\n  branch main\n  commit\n", &trunk()).unwrap();
        assert_eq!(graph.branches.len(), 2);
    }

    /// `mainBranchOrder`: the main branch takes its place among those that
    /// ask for one, as Mermaid orders them; one that asks for none stands
    /// before every whole number but 0.
    #[test]
    fn the_main_branch_stands_where_its_order_puts_it() {
        let source = "gitGraph\n  commit\n  branch late order: 3\n  branch early order: 1\n  branch plain\n  commit\n";
        let settings = Settings {
            main_order: 2.0,
            ..Settings::default()
        };
        let graph = from_mermaid_with(source, &settings).unwrap();
        let names: Vec<&str> = graph.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["plain", "early", "main", "late"]);
        // As Mermaid: a fraction of an order among those who ask for none,
        // the tenth beside the first.
        let settings = Settings {
            main_order: 0.15,
            ..Settings::default()
        };
        let many: String = (1..=11).map(|n| format!("  branch b{n}\n")).collect();
        let graph = from_mermaid_with(&format!("gitGraph\n  commit\n{many}"), &settings).unwrap();
        let names: Vec<&str> = graph.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "b1", "b10", "b11", "main", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9"
            ]
        );
        // A branch of order 0 stands after the main branch, before one
        // that asks for none, as in Mermaid.
        let graph =
            from_mermaid("gitGraph\n  commit\n  branch plain\n  branch first order: 0\n").unwrap();
        let names: Vec<&str> = graph.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["main", "first", "plain"]);
    }

    /// `showBranches` and `showCommitLabel`: without the lanes' names and
    /// lines, and without the ids; the tags stay.
    #[test]
    fn what_the_settings_hide_is_not_drawn() {
        let shown = draw(&from_mermaid_with(RELEASE, &trunk()).unwrap()).unwrap();
        for words in [
            ">trunk<",
            ">feature<",
            ">login-form<",
            ">v0.9<",
            "stroke-dasharray",
        ] {
            assert!(shown.contains(words), "{words}");
        }
        let hidden = Settings {
            show_branches: false,
            show_commit_label: false,
            ..trunk()
        };
        let svg = draw(&from_mermaid_with(RELEASE, &hidden).unwrap()).unwrap();
        for words in [
            ">trunk<",
            ">feature<",
            ">login-form<",
            ">initial-setup<",
            "stroke-dasharray",
        ] {
            assert!(!svg.contains(words), "{words}");
        }
        assert!(svg.contains(">v0.9<"));
    }

    /// `rotateCommitLabel`: the ids are turned by 45°, as in Mermaid. Where
    /// time runs to the right the turns stand as close as the tags let
    /// them; down the page a lane's ids stand in a column after its tags,
    /// rising to the right, and never meet.
    #[test]
    fn turned_ids_let_the_turns_stand_closer() {
        let width = |svg: &str| -> f32 {
            let at = svg.find("viewBox=\"").unwrap() + 9;
            svg[at..]
                .split_whitespace()
                .nth(2)
                .unwrap()
                .parse()
                .unwrap()
        };
        let level = draw(&from_mermaid_with(RELEASE, &trunk()).unwrap()).unwrap();
        let turned = Settings {
            rotate_commit_label: true,
            ..trunk()
        };
        let graph = from_mermaid_with(RELEASE, &turned).unwrap();
        assert_eq!(graph.direction, Direction::Across);
        let svg = draw(&graph).unwrap();
        assert_eq!(svg.matches("rotate(-45 ").count(), 3, "{svg}");
        assert!(
            width(&svg) < width(&level),
            "{} {}",
            width(&svg),
            width(&level)
        );
        for way in ["TB", "BT"] {
            let down = RELEASE.replace("gitGraph\n", &format!("gitGraph {way}:\n"));
            let svg = draw(&from_mermaid_with(&down, &turned).unwrap()).unwrap();
            assert_eq!(svg.matches("rotate(-45 ").count(), 3, "{way}");
        }
    }

    /// The front matter's settings of the `gitGraph` section the drawer
    /// carries out are no reason to leave the diagram to mmdr; another is.
    #[test]
    fn a_front_matter_of_the_settings_carried_out_is_read() {
        let front = |setting: &str| {
            format!(
                "---\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n    {setting}\n---\n{RELEASE}"
            )
        };
        assert!(from_mermaid_with(&front("rotateCommitLabel: true"), &trunk()).is_some());
        assert_eq!(
            mermaid_refusal(&front("parallelCommits: true"), &trunk()),
            Some((4, "a front matter setting"))
        );
    }
}
