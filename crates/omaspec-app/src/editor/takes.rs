//! What a command does in a tab that holds a Markdown document: works on
//! the document or on the diagram its pane shows, or, for a diagram file
//! only, says why not. Every command answers, so a new one cannot run on a
//! document's masked text or on all of its Markdown by mistake.

use super::Command;

impl Command {
    /// `Err` with what to say when the command works on a diagram file
    /// only and the tab holds a Markdown document.
    /// Whether a command may run with a project's file in the tab, whose
    /// pane shows the map made from the project's diagrams
    /// (docs/adr/0026): what reads or writes the text as a diagram, and
    /// what plays or folds one, may not.
    pub(super) fn in_a_project(self) -> Result<(), &'static str> {
        match self {
            Command::Skin(_)
            | Command::Convert
            | Command::Format
            | Command::Portable
            | Command::Prettify
            | Command::Fix
            | Command::FixAll
            | Command::ResetLine
            | Command::ResetLines
            | Command::LanguageDetected
            | Command::LanguagePlantUml
            | Command::LanguageMermaid
            | Command::RenameEverywhere
            | Command::ShowUses => {
                Err("the project's file is no diagram; this works in a diagram file")
            }
            Command::Compare
            | Command::CompareDiff
            | Command::CompareBehavior
            | Command::CompareSync
            | Command::CompareGlide => {
                Err("a project's map is made from its diagrams; compare one of them")
            }
            Command::Play
            | Command::PlayTrace
            | Command::PlayAsk(_)
            | Command::StopPlay
            | Command::FocusBox
            | Command::FoldBox
            | Command::ColorBox
            | Command::LabelSide
            | Command::UnfoldAll => Err("a map is not played or folded; open a diagram of it"),
            _ => Ok(()),
        }
    }

    pub(super) fn in_a_document(self) -> Result<(), &'static str> {
        match self {
            Command::Skin(_) => {
                Err("a skin pack goes into a diagram file, not into a Markdown document")
            }
            Command::Convert => Err(
                "writing it in the other language works on a diagram file, not on a Markdown document",
            ),
            Command::ResetLine | Command::ResetLines => {
                Err("lines by hand are drawn in a diagram file, not in a Markdown document")
            }
            Command::ColorBox => {
                Err("a box is colored in a diagram file, not in a Markdown document")
            }
            Command::LabelSide => {
                Err("a label takes its side in a diagram file, not in a Markdown document")
            }
            Command::LanguageDetected | Command::LanguagePlantUml | Command::LanguageMermaid => {
                Err("in a Markdown document, each fence names its diagram's language")
            }
            Command::CompareBehavior => {
                Err("the changes in behavior compare diagram files, not Markdown documents")
            }
            // The document itself: its text, its tab, its file; each of its
            // diagrams formatted in its place.
            Command::Save
            | Command::SaveAs
            | Command::Revert
            | Command::FindAll
            | Command::Find
            | Command::History
            | Command::ReplaceAll
            | Command::Compare
            | Command::StopCompare
            | Command::CompareDiff
            | Command::CompareSync
            | Command::CompareGlide
            | Command::Format
            | Command::Portable
            | Command::Prettify
            | Command::Collapse
            | Command::Expand
            | Command::CollapseAll
            | Command::ExpandAll
            | Command::CloseOthers
            | Command::CloseTab
            | Command::RenameEverywhere
            | Command::ShowUses
            | Command::Undo
            | Command::Redo
            | Command::Cut
            | Command::Copy
            | Command::Paste
            | Command::Motion
            | Command::JsonLayout
            | Command::StateTable
            | Command::Suggest
            | Command::Fix
            | Command::FixAll
            | Command::SelectAll => Ok(()),
            // The diagram its pane shows, as of a diagram file.
            Command::CopyPng
            | Command::CopySvg
            | Command::ExportSvg
            | Command::ExportPng
            | Command::ExportJpeg
            | Command::ExportPdf
            | Command::ExportMarkdown
            | Command::ExportZip
            | Command::RenderNow
            | Command::Detach
            | Command::FindInDiagram
            | Command::FollowTheme
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ActualSize
            | Command::Fit
            | Command::Play
            | Command::PlayTrace
            | Command::PlayAsk(_)
            | Command::StopPlay
            | Command::FocusBox
            | Command::FoldBox
            | Command::UnfoldAll
            | Command::Back => Ok(()),
            // The window, the project and the lists, whatever the tab holds.
            Command::Palette
            | Command::Choose
            | Command::ToggleMenuBar
            | Command::ToggleToolbar
            | Command::OpenMenu(_)
            | Command::StartFrom(_)
            | Command::Open
            | Command::NewProject
            | Command::Pick
            | Command::SwitchField
            | Command::NextLook
            | Command::ExportProject
            | Command::CheckProject
            | Command::OpenRecent
            | Command::NewTab
            | Command::NextTab
            | Command::PreviousTab
            | Command::ToggleSidebar
            | Command::Focus(_)
            | Command::Gallery
            | Command::OpenExample(_)
            | Command::NextGroup
            | Command::PreviousGroup
            | Command::OpenHelp
            | Command::LeaveHelp
            | Command::Quit
            | Command::Cancel
            | Command::Up
            | Command::Down
            | Command::Left
            | Command::Right => Ok(()),
        }
    }
}
