//! A panel floating over the window beside the widget it belongs to
//! (docs/adr/0025): a menu of the bar under its root, a child menu level
//! with its heading, the context menu at the pointer. iced lays the panel
//! out as an overlay, so its place comes from the layout and its size from
//! the panel itself; it moves left or up as far as needed to stay inside
//! the window. A press outside the panel tells the window to close it,
//! unless it lands on the widget and that is let through, as the bar's
//! roots are: a click on another root opens that one.

use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};

/// Where the panel goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    /// At this point of the window.
    At(Point),
    /// Under the `n`th of the widget's children, found below the chain of
    /// single children (a container around a row: the row's children).
    Below(usize),
    /// Right of the widget, `dx` from its right edge and `dy` from its top.
    Beside { dx: f32, dy: f32 },
}

/// Wraps `content`; while it has a panel, the panel floats beside it.
pub struct Anchored<'a, M> {
    content: Element<'a, M>,
    panel: Option<Element<'a, M>>,
    anchor: Anchor,
    /// Published on a press outside the panel.
    outside: Option<M>,
    /// A press on the content itself passes through to it.
    through: bool,
}

impl<'a, M> Anchored<'a, M> {
    pub fn new(
        content: impl Into<Element<'a, M>>,
        panel: Option<Element<'a, M>>,
        anchor: Anchor,
    ) -> Self {
        Self {
            content: content.into(),
            panel,
            anchor,
            outside: None,
            through: false,
        }
    }

    /// A press outside the panel publishes `message` and goes no further.
    pub fn on_outside(mut self, message: M) -> Self {
        self.outside = Some(message);
        self
    }

    /// A press on the content reaches the content, panel or not.
    pub fn through(mut self) -> Self {
        self.through = true;
        self
    }
}

/// Where a panel of `size` opens for `at` in a window of `window`: there,
/// or moved left or up as far as needed to stay inside, never past the
/// window's top left.
pub fn place(at: Point, size: Size, window: Size) -> Point {
    let x = if at.x + size.width > window.width {
        (window.width - size.width).max(0.0)
    } else {
        at.x
    };
    let y = if at.y + size.height > window.height {
        (at.y - size.height).max(0.0)
    } else {
        at.y
    };
    Point::new(x, y)
}

/// The bounds of the `n`th child below the chain of single children.
fn nth_child(layout: Layout<'_>, n: usize) -> Option<Rectangle> {
    let mut node = layout;
    loop {
        let mut children = node.children();
        let first = children.next()?;
        if children.next().is_none() {
            node = first;
            continue;
        }
        return node.children().nth(n).map(|child| child.bounds());
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for Anchored<'_, M> {
    fn children(&self) -> Vec<Tree> {
        let mut children = vec![Tree::new(&self.content)];
        if let Some(panel) = &self.panel {
            children.push(Tree::new(panel));
        }
        children
    }

    fn diff(&self, tree: &mut Tree) {
        let mut widgets: Vec<&dyn Widget<M, Theme, Renderer>> = vec![self.content.as_widget()];
        if let Some(panel) = &self.panel {
            widgets.push(panel.as_widget());
        }
        tree.diff_children(&widgets);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let (content_tree, panel_tree) = tree.children.split_at_mut(1);
        let base = self.content.as_widget_mut().overlay(
            &mut content_tree[0],
            layout,
            renderer,
            viewport,
            translation,
        );
        let (Some(panel), Some(panel_tree)) = (&mut self.panel, panel_tree.first_mut()) else {
            return base;
        };
        let bounds = layout.bounds() + translation;
        let at = match self.anchor {
            Anchor::At(at) => at,
            Anchor::Below(n) => {
                let root = nth_child(layout, n).map_or(bounds, |r| r + translation);
                Point::new(root.x, root.y + root.height)
            }
            Anchor::Beside { dx, dy } => Point::new(bounds.x + bounds.width + dx, bounds.y + dy),
        };
        let floating = overlay::Element::new(Box::new(Floating {
            panel,
            tree: panel_tree,
            at,
            outside: self.outside.clone(),
            through: self.through.then_some(bounds),
        }));
        Some(match base {
            Some(base) => overlay::Group::with_children(vec![base, floating]).overlay(),
            None => floating,
        })
    }
}

impl<'a, M: Clone + 'a> From<Anchored<'a, M>> for Element<'a, M> {
    fn from(anchored: Anchored<'a, M>) -> Self {
        Element::new(anchored)
    }
}

/// The panel as iced's overlay: laid out at its anchor, kept inside the
/// window, drawn over everything; a press outside it closes.
struct Floating<'a, 'b, M> {
    panel: &'b mut Element<'a, M>,
    tree: &'b mut Tree,
    at: Point,
    outside: Option<M>,
    /// Presses here reach the widget under the overlay.
    through: Option<Rectangle>,
}

impl<M: Clone> overlay::Overlay<M, Theme, Renderer> for Floating<'_, '_, M> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, bounds);
        let node = self
            .panel
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        let at = place(self.at, node.size(), bounds);
        node.move_to(at)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.panel.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.panel
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
    ) {
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && let Some(at) = cursor.position()
            && !layout.bounds().contains(at)
        {
            if self.through.is_some_and(|r| r.contains(at)) {
                return;
            }
            if let Some(message) = &self.outside {
                shell.publish(message.clone());
                shell.capture_event();
            }
            return;
        }
        self.panel.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.panel.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }

    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, M, Theme, Renderer>> {
        self.panel.as_widget_mut().overlay(
            self.tree,
            layout,
            renderer,
            &layout.bounds(),
            Vector::ZERO,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_stays_inside_the_window() {
        let window = Size::new(1000.0, 600.0);
        let size = Size::new(200.0, 300.0);
        // Room enough: where asked.
        assert_eq!(
            place(Point::new(100.0, 100.0), size, window),
            Point::new(100.0, 100.0)
        );
        // Near the right edge: moved left to the edge.
        assert_eq!(
            place(Point::new(900.0, 100.0), size, window),
            Point::new(800.0, 100.0)
        );
        // Near the bottom: opens upward from the point.
        assert_eq!(
            place(Point::new(100.0, 500.0), size, window),
            Point::new(100.0, 200.0)
        );
        // A window too small for it: the top left, not beyond.
        assert_eq!(
            place(Point::new(10.0, 10.0), size, Size::new(100.0, 100.0)),
            Point::ORIGIN
        );
    }
}
