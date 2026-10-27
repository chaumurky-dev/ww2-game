//! Pan/zoom maths for an SVG with `preserveAspectRatio="xMidYMid meet"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Zooming out may show up to this many world widths (the side copies fill the gap).
pub const MAX_ZOOM_OUT: f64 = 1.5;

/// Of the copies of `anchor_x` (shifted by whole worlds), the one nearest `centre_x`.
pub fn nearest_copy(anchor_x: f64, centre_x: f64, world_w: f64) -> f64 {
    [anchor_x - world_w, anchor_x, anchor_x + world_w]
        .into_iter()
        .min_by(|a, b| (a - centre_x).abs().total_cmp(&(b - centre_x).abs()))
        .unwrap_or(anchor_x)
}

impl ViewBox {
    pub fn new(v: [f64; 4]) -> Self {
        Self {
            x: v[0],
            y: v[1],
            w: v[2],
            h: v[3],
        }
    }

    pub fn attr(&self) -> String {
        format!("{:.2} {:.2} {:.2} {:.2}", self.x, self.y, self.w, self.h)
    }

    /// User units per CSS pixel for a container of `cw` x `ch` pixels.
    fn scale(&self, cw: f64, ch: f64) -> f64 {
        (self.w / cw.max(1.0)).max(self.h / ch.max(1.0))
    }

    /// Map a point in container pixels (relative to its top-left) to user (viewBox) units.
    pub fn to_user(&self, px: f64, py: f64, cw: f64, ch: f64) -> (f64, f64) {
        let s = self.scale(cw, ch);
        let (ox, oy) = ((cw - self.w / s) / 2.0, (ch - self.h / s) / 2.0);
        (self.x + (px - ox) * s, self.y + (py - oy) * s)
    }

    pub fn pan(&mut self, dx_px: f64, dy_px: f64, cw: f64, ch: f64) {
        let s = self.scale(cw, ch);
        self.x -= dx_px * s;
        self.y -= dy_px * s;
    }

    /// East-west wrap: keep `x` in `[base_x, base_x + world_w)`. The board is drawn in three
    /// identical copies, so this snap is invisible.
    pub fn wrap_x(&mut self, base_x: f64, world_w: f64) {
        if world_w > 0.0 {
            self.x = base_x + (self.x - base_x).rem_euclid(world_w);
        }
    }

    /// Does the horizontal span of this view touch the world copy starting at `copy_x`
    /// (copy covers `[copy_x, copy_x + world_w]`)?
    pub fn touches_copy(&self, copy_x: f64, world_w: f64) -> bool {
        self.x < copy_x + world_w && self.x + self.w > copy_x
    }

    /// Zoom by `factor` (>1 zooms out) keeping the point under (px, py) fixed.
    pub fn zoom_at(&mut self, factor: f64, px: f64, py: f64, cw: f64, ch: f64, base: &ViewBox) {
        let factor = factor.clamp(0.2, 5.0);
        let new_w = (self.w * factor).clamp(base.w / 10.0, base.w * MAX_ZOOM_OUT);
        let factor = new_w / self.w;
        let s = self.scale(cw, ch);
        let (ox, oy) = ((cw - self.w / s) / 2.0, (ch - self.h / s) / 2.0);
        let (ux, uy) = (self.x + (px - ox) * s, self.y + (py - oy) * s);
        self.w *= factor;
        self.h *= factor;
        let s2 = self.scale(cw, ch);
        let (ox2, oy2) = ((cw - self.w / s2) / 2.0, (ch - self.h / s2) / 2.0);
        self.x = ux - (px - ox2) * s2;
        self.y = uy - (py - oy2) * s2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vb(x: f64, w: f64) -> ViewBox {
        ViewBox {
            x,
            y: 0.0,
            w,
            h: w / 2.0,
        }
    }

    #[test]
    fn wraps_both_directions() {
        let mut v = vb(-100.0, 500.0);
        v.wrap_x(0.0, 2000.0);
        assert_eq!(v.x, 1900.0);
        let mut v = vb(2050.0, 500.0);
        v.wrap_x(0.0, 2000.0);
        assert_eq!(v.x, 50.0);
        let mut v = vb(2000.0, 500.0);
        v.wrap_x(0.0, 2000.0);
        assert_eq!(v.x, 0.0);
        let mut v = vb(1999.0, 500.0);
        v.wrap_x(0.0, 2000.0);
        assert_eq!(v.x, 1999.0);
        let mut v = vb(-4100.0, 500.0);
        v.wrap_x(0.0, 2000.0);
        assert_eq!(v.x, 1900.0);
    }

    #[test]
    fn nearest_copy_picks_shortest_pan() {
        // Hawaii at x=100 (just east of the seam), view centred on Japan (x=1800).
        assert_eq!(nearest_copy(100.0, 1800.0, 2000.0), 2100.0);
        assert_eq!(nearest_copy(1900.0, 100.0, 2000.0), -100.0);
        assert_eq!(nearest_copy(1000.0, 900.0, 2000.0), 1000.0);
    }

    #[test]
    fn touches_copy_only_when_overlapping() {
        let v = vb(1800.0, 500.0); // 1800..2300
        assert!(v.touches_copy(0.0, 2000.0));
        assert!(v.touches_copy(2000.0, 2000.0));
        assert!(!v.touches_copy(-2000.0, 2000.0));
    }

    #[test]
    fn zoom_clamps_and_keeps_point() {
        let base = vb(0.0, 2000.0);
        let mut v = base;
        v.zoom_at(5.0, 400.0, 300.0, 800.0, 400.0, &base);
        assert_eq!(v.w, 2000.0 * MAX_ZOOM_OUT);
        let mut v = base;
        v.zoom_at(0.2, 400.0, 200.0, 800.0, 400.0, &base);
        v.zoom_at(0.2, 400.0, 200.0, 800.0, 400.0, &base);
        v.zoom_at(0.2, 400.0, 200.0, 800.0, 400.0, &base);
        assert!((v.w - 200.0).abs() < 1e-6);
    }
}
