//! A grid whose cells are, row by row, as high as the tallest of their row: cards side by side
//! line up whatever they hold. Each cell is laid out once to measure it, then again at its row's
//! height (iced's own grid gives each cell the height of its content).

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

pub struct EvenGrid<'a, Message, Theme, Renderer> {
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    max_cell_width: f32,
    spacing: f32,
}

/// As many columns as fit cells at most `max_cell_width` wide, `spacing` apart.
pub fn even_grid<'a, Message, Theme, Renderer>(
    children: impl IntoIterator<Item = Element<'a, Message, Theme, Renderer>>,
    max_cell_width: f32,
    spacing: f32,
) -> EvenGrid<'a, Message, Theme, Renderer> {
    EvenGrid {
        children: children.into_iter().collect(),
        max_cell_width,
        spacing,
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for EvenGrid<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        let spacing = self.spacing;
        let columns =
            (((width + spacing) / (self.max_cell_width + spacing)).ceil() as usize).max(1);
        let cell = (width - spacing * (columns - 1) as f32) / columns as f32;
        let mut nodes = Vec::with_capacity(self.children.len());
        let mut y = 0.0;
        for (row, trees) in self
            .children
            .chunks_mut(columns)
            .zip(tree.children.chunks_mut(columns))
        {
            let free = layout::Limits::new(Size::new(cell, 0.0), Size::new(cell, f32::INFINITY));
            let height = row
                .iter_mut()
                .zip(trees.iter_mut())
                .map(|(child, tree)| {
                    child
                        .as_widget_mut()
                        .layout(tree, renderer, &free)
                        .size()
                        .height
                })
                .fold(0.0, f32::max);
            let even = layout::Limits::new(Size::new(cell, height), Size::new(cell, height));
            for (i, (child, tree)) in row.iter_mut().zip(trees.iter_mut()).enumerate() {
                nodes.push(
                    child
                        .as_widget_mut()
                        .layout(tree, renderer, &even)
                        .move_to(Point::new(i as f32 * (cell + spacing), y)),
                );
            }
            y += height + spacing;
        }
        let height = (y - spacing).max(0.0);
        layout::Node::with_children(Size::new(width, height), nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, state), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        for ((child, state), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<EvenGrid<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(grid: EvenGrid<'a, Message, Theme, Renderer>) -> Self {
        Self::new(grid)
    }
}
