//! One owner per finger. Geometry is shared with the native compositor.
pub mod layout;
pub mod ui;
use horizon_client::game::mobile::Action;
use layout::*;
use std::time::{Duration, Instant};
use winit::event::TouchPhase;

const SLOP: f64 = 8.;
const HOLD: Duration = Duration::from_millis(360);

#[derive(Clone, Copy)]
enum Owner {
    Move {
        origin: [f64; 2],
        rect: Rect,
    },
    World {
        origin: [f64; 2],
        last: [f64; 2],
        since: Instant,
        dragged: bool,
        mining: bool,
    },
    Button(usize),
    Edit {
        button: usize,
        offset: [f64; 2],
    },
    Ignore,
}
#[derive(Clone, Copy)]
struct Finger {
    id: u64,
    owner: Owner,
}
#[derive(Default)]
pub struct Controls {
    fingers: [Option<Finger>; 16],
    pub stick: [f32; 2],
    pub knob: [f64; 2],
    pub floating: Option<Rect>,
    held: [bool; 5],
    sneak: bool,
    sprint: bool,
    pub selected: Option<usize>,
}
const HELD: [usize; 5] = [JUMP, SNEAK, SPRINT, ATTACK, USE];
const BUTTONS: [Option<Action>; COUNT] = {
    let mut actions = [None; COUNT];
    actions[JUMP] = Some(Action::Jump);
    actions[SNEAK] = Some(Action::Sneak);
    actions[SPRINT] = Some(Action::Sprint);
    actions[ATTACK] = Some(Action::Attack);
    actions[USE] = Some(Action::Use);
    actions[INVENTORY] = Some(Action::Inventory);
    actions[PERSPECTIVE] = Some(Action::Perspective);
    actions[CHAT] = Some(Action::Chat);
    actions[PAUSE] = Some(Action::Pause);
    actions
};

#[derive(Default)]
pub struct Changes {
    pub edges: [Option<(Action, bool)>; 5],
    pub pulse: Option<Action>,
    pub look: [f64; 2],
    pub target: Option<Target>,
    pub layout_changed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Tap([f64; 2]),
    StartMine([f64; 2]),
    StopMine,
}

impl Controls {
    pub fn clear(&mut self) -> Changes {
        let mined = self
            .fingers
            .iter()
            .flatten()
            .any(|f| matches!(f.owner, Owner::World { mining: true, .. }));
        self.fingers.fill(None);
        self.stick = [0.; 2];
        self.knob = [0.; 2];
        self.floating = None;
        self.sneak = false;
        self.sprint = false;
        self.edges(Changes {
            target: mined.then_some(Target::StopMine),
            ..Changes::default()
        })
    }
    fn edges(&mut self, mut changes: Changes) -> Changes {
        for (i, button) in HELD.into_iter().enumerate() {
            let held = self.down(button);
            if held != self.held[i] {
                self.held[i] = held;
                changes.edges[i] = BUTTONS[button].map(|action| (action, held));
            }
        }
        changes
    }
    pub fn down(&self, button: usize) -> bool {
        match button {
            SNEAK => self.sneak,
            SPRINT => self.sprint,
            _ => self
                .fingers
                .iter()
                .flatten()
                .any(|f| matches!(f.owner, Owner::Button(b) if b == button)),
        }
    }
    pub fn advance(&mut self, now: Instant, settings: &Settings) -> Changes {
        if settings.scheme == Scheme::Crosshair {
            return Changes::default();
        }
        for finger in self.fingers.iter_mut().flatten() {
            if let Owner::World {
                origin,
                since,
                dragged: false,
                mining,
                ..
            } = &mut finger.owner
                && !*mining
                && now.saturating_duration_since(*since) >= HOLD
            {
                *mining = true;
                return Changes {
                    target: Some(Target::StartMine(*origin)),
                    ..Changes::default()
                };
            }
        }
        Changes::default()
    }

    pub fn touch(&mut self, event: Event, layout: &Layout, settings: &mut Settings) -> Changes {
        let Event {
            id,
            phase,
            point: p,
            slot,
            now,
            editor,
        } = event;
        let mut out = Changes::default();
        let index = self
            .fingers
            .iter()
            .position(|f| f.is_some_and(|f| f.id == id));
        if phase == TouchPhase::Started {
            if index.is_some() {
                return out;
            }
            let Some(index) = self.fingers.iter().position(Option::is_none) else {
                return out;
            };
            let hit = if self.floating.is_some_and(|r| r.contains(p)) {
                Some(MOVE)
            } else {
                layout.hit(p)
            };
            let owner = if editor {
                if let Some(button) = hit {
                    let [x, y] = layout.buttons[button].center();
                    self.selected = Some(button);
                    if self
                        .fingers
                        .iter()
                        .flatten()
                        .any(|f| matches!(f.owner, Owner::Edit { .. }))
                    {
                        return Changes::default();
                    }
                    Owner::Edit {
                        button,
                        offset: [x - p[0], y - p[1]],
                    }
                } else {
                    Owner::Ignore
                }
            } else if let Some(slot) = slot {
                out.pulse = Some(Action::SelectHotbar(slot));
                Owner::Ignore
            } else if !layout.safe.contains(p) || layout.hotbar.contains(p) {
                Owner::Ignore
            } else if hit == Some(MOVE)
                || (hit.is_none()
                    && settings.dynamic
                    && settings.scheme != Scheme::DpadTap
                    && layout.floating_move(p, settings.left_handed).is_some())
            {
                if self
                    .fingers
                    .iter()
                    .flatten()
                    .any(|f| matches!(f.owner, Owner::Move { .. }))
                {
                    Owner::Ignore
                } else {
                    let dynamic = settings.dynamic && settings.scheme != Scheme::DpadTap;
                    let rect = if dynamic {
                        layout
                            .floating_move(p, settings.left_handed)
                            .unwrap_or(layout.buttons[MOVE])
                    } else {
                        layout.buttons[MOVE]
                    };
                    self.floating = dynamic.then_some(rect);
                    let origin = if dynamic { p } else { rect.center() };
                    self.move_to(origin, rect, p, settings);
                    Owner::Move { origin, rect }
                }
            } else if let Some(button) = hit {
                let toggle = match button {
                    SNEAK => Some(&mut self.sneak),
                    SPRINT => Some(&mut self.sprint),
                    _ => None,
                };
                if let Some(toggle) = toggle {
                    *toggle = !*toggle;
                    Owner::Ignore
                } else {
                    Owner::Button(button)
                }
            } else if self
                .fingers
                .iter()
                .flatten()
                .any(|f| matches!(f.owner, Owner::World { .. }))
            {
                Owner::Ignore
            } else {
                Owner::World {
                    origin: p,
                    last: p,
                    since: now,
                    dragged: false,
                    mining: false,
                }
            };
            self.fingers[index] = Some(Finger { id, owner });
        } else if let Some(index) = index {
            let mut f = self.fingers[index].take().unwrap();
            match &mut f.owner {
                Owner::Move { origin, rect } => {
                    if phase == TouchPhase::Moved {
                        self.move_to(*origin, *rect, p, settings);
                    } else {
                        self.stick = [0.; 2];
                        self.knob = [0.; 2];
                        self.floating = None;
                    }
                }
                Owner::World {
                    origin,
                    last,
                    since,
                    dragged,
                    mining,
                } => {
                    let outside = (p[0] - origin[0]).hypot(p[1] - origin[1]) >= SLOP;
                    if phase == TouchPhase::Moved && (*dragged || outside) {
                        let previous = if *dragged { *last } else { *origin };
                        *dragged = true;
                        let speed = settings.sensitivity * 2.;
                        out.look = [
                            (p[0] - previous[0]) * speed,
                            (p[1] - previous[1]) * speed * if settings.invert { -1. } else { 1. },
                        ];
                        *last = p;
                    } else if phase != TouchPhase::Moved {
                        out.target = if *mining {
                            Some(Target::StopMine)
                        } else if phase == TouchPhase::Ended
                            && !*dragged
                            && !outside
                            && now.saturating_duration_since(*since) < HOLD
                            && settings.scheme != Scheme::Crosshair
                        {
                            Some(Target::Tap(*origin))
                        } else {
                            None
                        };
                    }
                }
                Owner::Button(button) if *button >= INVENTORY && phase == TouchPhase::Ended => {
                    if layout.buttons[*button].contains(p) {
                        out.pulse = BUTTONS[*button];
                    }
                }
                Owner::Edit { button, offset } if editor && phase == TouchPhase::Moved => {
                    let pos = layout
                        .placement_bounds()
                        .normalized([p[0] + offset[0], p[1] + offset[1]], settings.left_handed);
                    settings.placed()[*button].center = Some(pos);
                    out.layout_changed = true;
                }
                _ => {}
            }
            if phase == TouchPhase::Moved {
                self.fingers[index] = Some(f);
            }
        }
        self.edges(out)
    }
    fn move_to(&mut self, origin: [f64; 2], rect: Rect, p: [f64; 2], settings: &Settings) {
        let r = (rect.w * 0.39).max(1.);
        let [x, y] = [(p[0] - origin[0]) / r, (p[1] - origin[1]) / r];
        let magnitude = x.hypot(y);
        let norm = magnitude.max(1.);
        self.knob = [x / norm, y / norm];
        self.stick = if settings.scheme == Scheme::DpadTap {
            let sector = |v: f64| {
                if v.abs() > 0.3827 * magnitude && magnitude >= 0.28 {
                    -v.signum() as f32
                } else {
                    0.
                }
            };
            let [x, y] = [sector(x), sector(y)];
            let norm = x.hypot(y).max(1.);
            [x / norm, y / norm]
        } else if magnitude > settings.deadzone {
            let amp =
                (magnitude.min(1.) - settings.deadzone) / ((1. - settings.deadzone) * magnitude);
            [(-x * amp) as f32, (-y * amp) as f32]
        } else {
            [0.; 2]
        };
    }
}

pub struct Event {
    pub id: u64,
    pub phase: TouchPhase,
    pub point: [f64; 2],
    pub slot: Option<usize>,
    pub now: Instant,
    pub editor: bool,
}

#[cfg(test)]
mod tests;
