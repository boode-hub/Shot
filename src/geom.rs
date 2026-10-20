//! Points and integer rectangles in physical pixels.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn dist(self, o: Pt) -> f32 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
}

/// Axis-aligned rectangle; `w`/`h` are never negative once normalised.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// Rectangle spanning two corner points, in either order.
    pub fn from_points(a: Pt, b: Pt) -> Self {
        let x0 = a.x.min(b.x).round() as i32;
        let y0 = a.y.min(b.y).round() as i32;
        let x1 = a.x.max(b.x).round() as i32;
        let y1 = a.y.max(b.y).round() as i32;
        Self::new(x0, y0, x1 - x0, y1 - y0)
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x as f32 && p.x < self.right() as f32 && p.y >= self.y as f32 && p.y < self.bottom() as f32
    }

    pub fn intersect(&self, o: &IRect) -> IRect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        IRect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    /// Clamp a point inside the rectangle (inclusive of the far edge).
    pub fn clamp(&self, p: Pt) -> Pt {
        Pt::new(p.x.clamp(self.x as f32, self.right() as f32), p.y.clamp(self.y as f32, self.bottom() as f32))
    }

    pub fn to_skia(&self) -> Option<tiny_skia::Rect> {
        tiny_skia::Rect::from_xywh(self.x as f32, self.y as f32, self.w as f32, self.h as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_points_normalises() {
        let r = IRect::from_points(Pt::new(50.0, 40.0), Pt::new(10.0, 20.0));
        assert_eq!(r, IRect::new(10, 20, 40, 20));
    }

    #[test]
    fn intersect_disjoint_is_empty() {
        let a = IRect::new(0, 0, 10, 10);
        let b = IRect::new(20, 20, 5, 5);
        assert!(a.intersect(&b).is_empty());
        assert_eq!(a.intersect(&IRect::new(5, 5, 10, 10)), IRect::new(5, 5, 5, 5));
    }
}
