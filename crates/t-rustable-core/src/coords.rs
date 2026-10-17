//! PDF to display coordinates (ARCHITECTURE section 4).
//!
//! Display coordinates are pixels from the top left of the crop box, after the page
//! rotation and the zoom. PDF coordinates are points in user space, before rotation.

use t_rustable_engine::{Point, Rect};

/// How one page is shown: its crop box, its rotation, and pixels per point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageView {
    pub crop_box: Rect,
    /// Quarter turns clockwise.
    pub quarter_turns: u8,
    pub scale: f64,
}

impl PageView {
    pub fn to_display(&self, p: Point) -> Point {
        let Rect {
            left: l,
            bottom: b,
            right: r,
            top: t,
        } = self.crop_box;
        let (x, y) = match self.quarter_turns % 4 {
            0 => (p.x - l, t - p.y),
            1 => (p.y - b, p.x - l),
            2 => (r - p.x, p.y - b),
            _ => (t - p.y, r - p.x),
        };
        Point {
            x: x * self.scale,
            y: y * self.scale,
        }
    }

    pub fn from_display(&self, d: Point) -> Point {
        let Rect {
            left: l,
            bottom: b,
            right: r,
            top: t,
        } = self.crop_box;
        let (x, y) = (d.x / self.scale, d.y / self.scale);
        let (px, py) = match self.quarter_turns % 4 {
            0 => (l + x, t - y),
            1 => (l + y, b + x),
            2 => (r - x, b + y),
            _ => (r - y, t - x),
        };
        Point { x: px, y: py }
    }

    /// Size of the shown page in pixels.
    pub fn display_size(&self) -> (f64, f64) {
        let (w, h) = (self.crop_box.width(), self.crop_box.height());
        let (w, h) = if self.quarter_turns % 2 == 1 {
            (h, w)
        } else {
            (w, h)
        };
        (w * self.scale, h * self.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CROP: Rect = Rect {
        left: 10.0,
        bottom: 20.0,
        right: 110.0,
        top: 220.0,
    };

    fn view(q: u8) -> PageView {
        PageView {
            crop_box: CROP,
            quarter_turns: q,
            scale: 2.0,
        }
    }

    fn pt(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn corners_go_where_the_rotation_puts_them() {
        let bottom_left = pt(10.0, 20.0);
        let top_left = pt(10.0, 220.0);
        // No rotation: the top left corner is at the display origin.
        assert_eq!(view(0).to_display(top_left), pt(0.0, 0.0));
        assert_eq!(view(0).to_display(bottom_left), pt(0.0, 400.0));
        // A clockwise turn moves the bottom left corner to the top left.
        assert_eq!(view(1).to_display(bottom_left), pt(0.0, 0.0));
        assert_eq!(view(1).to_display(top_left), pt(400.0, 0.0));
        assert_eq!(view(2).to_display(pt(110.0, 20.0)), pt(0.0, 0.0));
        assert_eq!(view(3).to_display(pt(110.0, 220.0)), pt(0.0, 0.0));
        assert_eq!(view(1).display_size(), (400.0, 200.0));
    }

    #[test]
    fn from_display_inverts_to_display() {
        for q in 0..4 {
            for p in [pt(10.0, 20.0), pt(37.5, 101.25), pt(110.0, 220.0)] {
                let back = view(q).from_display(view(q).to_display(p));
                assert!(
                    (back.x - p.x).abs() < 1e-9 && (back.y - p.y).abs() < 1e-9,
                    "{q}: {p:?}"
                );
            }
        }
    }
}
