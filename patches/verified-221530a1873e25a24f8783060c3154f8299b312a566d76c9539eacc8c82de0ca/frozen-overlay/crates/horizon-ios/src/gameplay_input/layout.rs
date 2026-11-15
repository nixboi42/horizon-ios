use serde_json::{Value, json};

pub const COUNT: usize = 10;
pub const MOVE: usize = 0;
pub const JUMP: usize = 1;
pub const SNEAK: usize = 2;
pub const SPRINT: usize = 3;
pub const ATTACK: usize = 4;
pub const USE: usize = 5;
pub const INVENTORY: usize = 6;
pub const PERSPECTIVE: usize = 7;
pub const CHAT: usize = 8;
pub const PAUSE: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scheme {
    #[default]
    Crosshair,
    JoystickTap,
    DpadTap,
}
impl Scheme {
    pub fn number(self) -> usize {
        match self {
            Self::Crosshair => 0,
            Self::JoystickTap => 1,
            Self::DpadTap => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub center: Option<[f64; 2]>,
    pub size: f64,
    pub opacity: f64,
}
impl Default for Placement {
    fn default() -> Self {
        Self {
            center: None,
            size: 1.,
            opacity: 1.,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub scheme: Scheme,
    pub sensitivity: f64,
    pub invert: bool,
    pub deadzone: f64,
    pub dynamic: bool,
    pub size: f64,
    pub opacity: f64,
    pub left_handed: bool,
    pub layouts: [[Placement; COUNT]; 3],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            scheme: Scheme::Crosshair,
            sensitivity: 0.5,
            invert: false,
            deadzone: 0.14,
            dynamic: false,
            size: 1.,
            opacity: 0.72,
            left_handed: false,
            layouts: [[Placement::default(); COUNT]; 3],
        }
    }
}
impl Settings {
    pub fn set(&mut self, key: usize, value: f64) {
        if !value.is_finite() {
            return;
        }
        match key {
            0 => {
                self.scheme = match value as usize {
                    1 => Scheme::JoystickTap,
                    2 => Scheme::DpadTap,
                    _ => Scheme::Crosshair,
                }
            }
            1 => self.sensitivity = value.clamp(0.1, 1.),
            2 => self.invert = value != 0.,
            3 => self.deadzone = value.clamp(0., 0.45),
            4 => self.dynamic = value != 0.,
            6 => self.size = value.clamp(0.75, 1.4),
            7 => self.opacity = value.clamp(0.3, 1.),
            8 => self.left_handed = value != 0.,
            _ => {}
        }
    }
    pub fn values(&self) -> [f64; 9] {
        [
            self.scheme.number() as f64,
            self.sensitivity,
            f64::from(self.invert),
            self.deadzone,
            f64::from(self.dynamic),
            1., // Legacy sneak setting slot in saved settings and the UIKit bridge.
            self.size,
            self.opacity,
            f64::from(self.left_handed),
        ]
    }
    pub fn placed(&mut self) -> &mut [Placement; COUNT] {
        &mut self.layouts[self.scheme.number()]
    }
    pub fn encode(&self) -> Value {
        json!({
            "version": 1, "settings": self.values(),
            "layouts": self.layouts.map(|layout| layout.map(|p| json!([p.center, p.size, p.opacity]))),
        })
    }
    pub fn decode(data: &Value) -> Self {
        let mut result = Self::default();
        if data["version"] != 1 {
            return result;
        }
        for i in 0..9 {
            if let Some(value) = data["settings"][i].as_f64() {
                result.set(i, value);
            }
        }
        for (i, layout) in result.layouts.iter_mut().enumerate() {
            for (j, placement) in layout.iter_mut().enumerate() {
                let entry = &data["layouts"][i][j];
                placement.center = entry[0][0]
                    .as_f64()
                    .zip(entry[0][1].as_f64())
                    .filter(|(x, y)| (0. ..=1.).contains(x) && (0. ..=1.).contains(y))
                    .map(|(x, y)| [x, y]);
                placement.size = entry[1].as_f64().unwrap_or(1.).clamp(0.75, 1.4);
                placement.opacity = entry[2].as_f64().unwrap_or(1.).clamp(0.4, 1.);
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl Rect {
    pub fn contains(self, [x, y]: [f64; 2]) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn intersects(self, r: Self) -> bool {
        self.w > 0.
            && self.h > 0.
            && r.w > 0.
            && r.h > 0.
            && self.x < r.x + r.w
            && r.x < self.x + self.w
            && self.y < r.y + r.h
            && r.y < self.y + self.h
    }
    pub fn inset(self, margin: f64) -> Self {
        Self {
            x: self.x + margin,
            y: self.y + margin,
            w: (self.w - 2. * margin).max(0.),
            h: (self.h - 2. * margin).max(0.),
        }
    }
    pub fn center(self) -> [f64; 2] {
        [self.x + self.w / 2., self.y + self.h / 2.]
    }
    pub fn square(self, [x, y]: [f64; 2], d: f64) -> Self {
        let d = d.min(self.w).min(self.h);
        Self {
            x: (x - d / 2.).clamp(self.x, self.x + self.w - d),
            y: (y - d / 2.).clamp(self.y, self.y + self.h - d),
            w: d,
            h: d,
        }
    }
    pub fn normalized(self, [x, y]: [f64; 2], mirror: bool) -> [f64; 2] {
        let x = ((x - self.x) / self.w.max(1.)).clamp(0., 1.);
        [
            if mirror { 1. - x } else { x },
            ((y - self.y) / self.h.max(1.)).clamp(0., 1.),
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layout {
    pub safe: Rect,
    pub hotbar: Rect,
    pub buttons: [Rect; COUNT],
    pub opacity: [f64; COUNT],
}
impl Layout {
    pub fn resolve(settings: &Settings, safe: Rect, hotbar: Rect) -> Self {
        let mut result = Self {
            safe,
            hotbar,
            ..Self::default()
        };
        let bounds = safe.inset(6.);
        if bounds.w < 150. || bounds.h < 144. {
            return result;
        }
        // The editor's toolbar occupies a top strip. Share that exclusion with
        // gameplay so editing previews the exact same positions and sizes.
        let b = result.placement_bounds();
        let [cx, _] = b.center();
        let x = b.x;
        let y = b.y;
        let bottom = y + b.h;
        let right = x + b.w;
        let unit = (b.h * 0.15).clamp(38., 63.);
        let stick = unit * 2.;
        let home = [
            [x + stick * 0.65, bottom - stick * 0.72],
            [right - unit * 0.8, bottom - unit * 1.55],
            [right - unit * 1.9, bottom - unit * 0.65],
            [x + unit * 3.0, bottom - unit * 2.25],
            [right - unit * 2.0, bottom - unit * 2.65],
            [right - unit * 0.7, bottom - unit * 2.8],
            [right - unit * 0.68, y + unit * 0.65],
            [x + unit * 0.68, y + unit * 0.65],
            [cx - unit * 0.64, y.max(76.) + unit * 0.55],
            [cx + unit * 0.64, y.max(76.) + unit * 0.55],
        ];
        let bar = hotbar.inset(-6.);
        // Launcher "Return" owns the top-center 210×76 points in gameplay.
        // Reserve the same area while editing so previews match the live HUD.
        let launcher = Rect {
            x: safe.center()[0] - 110.,
            y: 0.,
            w: 220.,
            h: 76.,
        };
        for (i, mut p) in home.into_iter().enumerate() {
            if i == ATTACK && settings.scheme != Scheme::Crosshair {
                continue;
            }
            let placement = settings.layouts[settings.scheme.number()][i];
            if let Some([nx, ny]) = placement.center {
                p = [b.x + nx * b.w, b.y + ny * b.h];
            }
            if settings.left_handed {
                p[0] = right - (p[0] - x);
            }
            let d = ((if i == MOVE { stick } else { unit }) * placement.size * settings.size)
                .max(if i == MOVE { 70. } else { 42. });
            let fits = |r: Rect| {
                !r.intersects(bar)
                    && !r.intersects(launcher)
                    && !result.buttons[..i]
                        .iter()
                        .any(|other| r.intersects(other.inset(-3.)) && other.w > 0.)
            };
            let mut chosen = b.square(p, d);
            if !fits(chosen) {
                // Keep the requested center if it fits. Otherwise pick the closest
                // free grid position without hiding controls under other controls.
                let mut best = f64::INFINITY;
                let step = 12.;
                let step_count = |extent: f64| ((extent / step).ceil() as usize).min(150);
                for smaller in [d, (d * 0.8).max(42.), 42.] {
                    for row in 0..=step_count(b.h) {
                        for col in 0..=step_count(b.w) {
                            let r = b.square(
                                [b.x + col as f64 * step, b.y + row as f64 * step],
                                smaller,
                            );
                            if !fits(r) {
                                continue;
                            }
                            let [rx, ry] = r.center();
                            let cost = (rx - p[0]).powi(2) + (ry - p[1]).powi(2);
                            if cost < best {
                                chosen = r;
                                best = cost;
                            }
                        }
                    }
                    if best.is_finite() {
                        break;
                    }
                }
                if !best.is_finite() {
                    continue;
                }
            }
            result.buttons[i] = chosen;
            result.opacity[i] = (settings.opacity * placement.opacity).max(0.26);
        }
        result
    }
    pub fn hit(&self, p: [f64; 2]) -> Option<usize> {
        self.buttons.iter().position(|r| r.contains(p))
    }
    pub fn placement_bounds(&self) -> Rect {
        let b = self.safe.inset(6.);
        Rect {
            y: b.y + 45.,
            h: (b.h - 45.).max(0.),
            ..b
        }
    }
    pub fn floating_move(&self, p: [f64; 2], left_handed: bool) -> Option<Rect> {
        let b = self.placement_bounds();
        let [nx, ny] = b.normalized(p, left_handed);
        if !b.contains(p) || nx > 0.31 || ny < 0.48 {
            return None;
        }
        let r = b.square(p, self.buttons[MOVE].w);
        (r.w > 0.
            && !r.intersects(self.hotbar)
            && !self.buttons[1..].iter().any(|button| r.intersects(*button)))
        .then_some(r)
    }
}
