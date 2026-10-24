//! Pan/zoom maths for an SVG with `preserveAspectRatio="xMidYMid meet"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
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

    pub fn pan(&mut self, dx_px: f64, dy_px: f64, cw: f64, ch: f64) {
        let s = self.scale(cw, ch);
        self.x -= dx_px * s;
        self.y -= dy_px * s;
    }

    /// Zoom by `factor` (>1 zooms out) keeping the point under (px, py) fixed.
    pub fn zoom_at(&mut self, factor: f64, px: f64, py: f64, cw: f64, ch: f64, base: &ViewBox) {
        let factor = factor.clamp(0.2, 5.0);
        let new_w = (self.w * factor).clamp(base.w / 10.0, base.w * 1.5);
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
