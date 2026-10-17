//! Small helpers for points, rectangles, and colors. All values are PDF points.

use crate::types::{Color, Point, Rect};

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Rect {
    /// The rectangle between two corners, in any order.
    pub fn from_corners(a: Point, b: Point) -> Self {
        Self {
            left: a.x.min(b.x),
            bottom: a.y.min(b.y),
            right: a.x.max(b.x),
            top: a.y.max(b.y),
        }
    }

    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.top - self.bottom
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.left && p.x <= self.right && p.y >= self.bottom && p.y <= self.top
    }
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_from_corners() {
        let r = Rect::from_corners(Point::new(10.0, 50.0), Point::new(2.0, 20.0));
        assert_eq!((r.left, r.bottom, r.right, r.top), (2.0, 20.0, 10.0, 50.0));
        assert_eq!((r.width(), r.height()), (8.0, 30.0));
        assert!(r.contains(Point::new(5.0, 25.0)));
        assert!(!r.contains(Point::new(11.0, 25.0)));
    }
}
