//! Popover ancorado (§6 "Popover"): abre abaixo do âncora (ou acima, se estourar), alinhado à esquerda ou à direita.
//! É um widget que desenha o conteúdo numa camada de sobreposição, então nada o corta nem o cobre.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::mouse;
use iced::{Element, Event, Length, Rectangle, Size, Vector};

#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    Left,
    Right,
}

pub struct Popover<'a, M, T, R> {
    base: Element<'a, M, T, R>,
    content: Element<'a, M, T, R>,
    open: bool,
    side: Side,
    prefer_above: bool,
    gap: f32,
    x_off: f32,
    on_dismiss: Option<M>,
}

impl<'a, M, T, R> Popover<'a, M, T, R> {
    pub fn new(base: impl Into<Element<'a, M, T, R>>, content: impl Into<Element<'a, M, T, R>>, open: bool) -> Self {
        Self { base: base.into(), content: content.into(), open, side: Side::Left, prefer_above: false, gap: 6.0, x_off: 0.0, on_dismiss: None }
    }
    pub fn side(mut self, s: Side) -> Self {
        self.side = s;
        self
    }
    pub fn above(mut self, v: bool) -> Self {
        self.prefer_above = v;
        self
    }
    pub fn offset_x(mut self, x: f32) -> Self {
        self.x_off = x;
        self
    }
    pub fn on_dismiss(mut self, m: M) -> Self {
        self.on_dismiss = Some(m);
        self
    }
}

impl<'a, M: Clone + 'a, T: 'a, R: iced::advanced::Renderer + 'a> Widget<M, T, R> for Popover<'a, M, T, R> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.base), Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.base, &self.content]);
    }
    fn size(&self) -> Size<Length> {
        self.base.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.base.as_widget().size_hint()
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &R, limits: &layout::Limits) -> layout::Node {
        self.base.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }
    fn draw(&self, tree: &Tree, renderer: &mut R, theme: &T, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        self.base.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }
    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &R, op: &mut dyn widget::Operation) {
        self.base.as_widget_mut().operate(&mut tree.children[0], layout, renderer, op);
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &R, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle) {
        self.base.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }
    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &R) -> mouse::Interaction {
        self.base.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }
    fn overlay<'b>(&'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &R, viewport: &Rectangle, translation: Vector) -> Option<overlay::Element<'b, M, T, R>> {
        let mut children = tree.children.iter_mut();
        let base_tree = children.next()?;
        let content_tree = children.next()?;
        let base_overlay = self.base.as_widget_mut().overlay(base_tree, layout, renderer, viewport, translation);
        let pop = if self.open {
            Some(overlay::Element::new(Box::new(PopOverlay {
                content: &mut self.content,
                tree: content_tree,
                anchor: layout.bounds() + translation,
                side: self.side,
                prefer_above: self.prefer_above,
                gap: self.gap,
                x_off: self.x_off,
                on_dismiss: self.on_dismiss.clone(),
            })))
        } else {
            None
        };
        if base_overlay.is_some() || pop.is_some() {
            Some(overlay::Group::with_children(base_overlay.into_iter().chain(pop).collect()).overlay())
        } else {
            None
        }
    }
}

struct PopOverlay<'a, 'b, M, T, R> {
    content: &'b mut Element<'a, M, T, R>,
    tree: &'b mut Tree,
    anchor: Rectangle,
    side: Side,
    prefer_above: bool,
    gap: f32,
    x_off: f32,
    on_dismiss: Option<M>,
}

impl<M: Clone, T, R: iced::advanced::Renderer> overlay::Overlay<M, T, R> for PopOverlay<'_, '_, M, T, R> {
    fn layout(&mut self, renderer: &R, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, Size::new(bounds.width - 24.0, bounds.height - 24.0));
        let node = self.content.as_widget_mut().layout(self.tree, renderer, &limits);
        let size = node.size();
        let mut x = match self.side {
            Side::Left => self.anchor.x + self.x_off,
            Side::Right => self.anchor.x + self.anchor.width - size.width + self.x_off,
        };
        x = x.clamp(12.0, (bounds.width - size.width - 12.0).max(12.0));
        let below = self.anchor.y + self.anchor.height + self.gap;
        let above = self.anchor.y - self.gap - size.height;
        let fits_below = below + size.height <= bounds.height - 12.0;
        let fits_above = above >= 12.0;
        let y = if self.prefer_above { if fits_above { above } else { below } } else if fits_below { below } else if fits_above { above } else { below };
        let y = y.clamp(12.0, (bounds.height - size.height - 12.0).max(12.0));
        layout::Node::with_children(size, vec![node]).move_to(iced::Point::new(x, y))
    }
    fn draw(&self, renderer: &mut R, theme: &T, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor) {
        self.content.as_widget().draw(self.tree, renderer, theme, style, layout.children().next().unwrap(), cursor, &Rectangle::with_size(Size::INFINITE));
    }
    fn update(&mut self, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &R, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>) {
        let inner = layout.children().next().unwrap();
        self.content.as_widget_mut().update(self.tree, event, inner, cursor, renderer, clipboard, shell, &Rectangle::with_size(Size::INFINITE));
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event {
            if !shell.is_event_captured() && !cursor.is_over(layout.bounds()) && !cursor.is_over(self.anchor) {
                if let Some(m) = &self.on_dismiss {
                    shell.publish(m.clone());
                }
                shell.capture_event();
            }
        }
        if let Event::Keyboard(iced::keyboard::Event::KeyPressed { key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape), .. }) = event {
            if let Some(m) = &self.on_dismiss {
                shell.publish(m.clone());
                shell.capture_event();
            }
        }
    }
    fn mouse_interaction(&self, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &R) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(self.tree, layout.children().next().unwrap(), cursor, &Rectangle::with_size(Size::INFINITE), renderer)
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &R, op: &mut dyn widget::Operation) {
        self.content.as_widget_mut().operate(self.tree, layout.children().next().unwrap(), renderer, op);
    }
}

impl<'a, M: Clone + 'a, T: 'a, R: iced::advanced::Renderer + 'a> From<Popover<'a, M, T, R>> for Element<'a, M, T, R> {
    fn from(p: Popover<'a, M, T, R>) -> Self {
        Element::new(p)
    }
}
