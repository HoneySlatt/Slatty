//! Sizes, positions and scroll positions as the state keeps them, of Slatty's own: the state does
//! not depend on the interface framework, whose types the interface turns into these.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Size { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// Where a scrolling area is scrolled to, how large it shows, and how large its content is.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scroll {
    pub offset: Point,
    pub shown: Size,
    pub content: Size,
}

impl Scroll {
    /// How far it is scrolled, from the top left.
    pub fn absolute_offset(&self) -> Point {
        self.offset
    }

    /// The size of the part that shows.
    pub fn bounds(&self) -> Size {
        self.shown
    }

    pub fn content_bounds(&self) -> Size {
        self.content
    }
}
