//! Window shortcuts that win over the widgets inside: the editor widget
//! binds Ctrl+Enter itself (new line below), but here it renders. The
//! right mouse button too, where a wrapper asks for it: the editor's
//! context menu (docs/adr/0025).

use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::keyboard::{self, Key, Modifiers};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};

/// The message for a key press, if it is a shortcut.
type Shortcuts<'a, M> = Box<dyn Fn(&Key, Modifiers) -> Option<M> + 'a>;

/// The message for the right mouse button at this point of the window.
type RightClick<'a, M> = Box<dyn Fn(Point) -> M + 'a>;

/// Wraps `content`; key presses for which `shortcut` returns a message
/// publish it and go no further, and so does the right mouse button over
/// the content when [`Guard::on_right_click`] is set.
pub struct Guard<'a, M> {
    content: Element<'a, M>,
    shortcut: Shortcuts<'a, M>,
    right_click: Option<RightClick<'a, M>>,
}

impl<'a, M> Guard<'a, M> {
    pub fn new(
        content: impl Into<Element<'a, M>>,
        shortcut: impl Fn(&Key, Modifiers) -> Option<M> + 'a,
    ) -> Self {
        Self {
            content: content.into(),
            shortcut: Box::new(shortcut),
            right_click: None,
        }
    }

    /// The right mouse button over the content publishes `message` with
    /// the pointer's place in the window, and the content never sees it.
    pub fn on_right_click(mut self, message: impl Fn(Point) -> M + 'a) -> Self {
        self.right_click = Some(Box::new(message));
        self
    }
}

impl<M> Widget<M, Theme, Renderer> for Guard<'_, M> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
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
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
            && let Some(message) = (self.shortcut)(key, *modifiers)
        {
            shell.publish(message);
            shell.capture_event();
            return;
        }
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && let Some(right_click) = &self.right_click
            && let Some(at) = cursor.position_over(layout.bounds())
        {
            shell.publish(right_click(at));
            shell.capture_event();
            return;
        }
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
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a> From<Guard<'a, M>> for Element<'a, M> {
    fn from(guard: Guard<'a, M>) -> Self {
        Element::new(guard)
    }
}
