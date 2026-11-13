//! Temporary touch ownership and action edges. All simulation stays in Game.
use horizon_client::game::mobile::Action;
use std::collections::BTreeMap;
use winit::event::TouchPhase;

const HELD: [Action; 7] = [
    Action::Forward,
    Action::Back,
    Action::Left,
    Action::Right,
    Action::Jump,
    Action::Attack,
    Action::Use,
];
#[derive(Clone, Copy)]
enum Touch {
    Move { origin: [f64; 2], delta: [f64; 2] },
    Look([f64; 2]),
    Button(Action),
    Hotbar,
    Tap,
}
#[derive(Default)]
pub struct Controls {
    touches: BTreeMap<u64, Touch>,
    held: [bool; 7],
}
#[derive(Default)]
pub struct Changes {
    pub actions: Vec<(Action, bool)>,
    pub look: [f64; 2],
}
impl Controls {
    pub fn clear(&mut self) -> Changes {
        self.touches.clear();
        self.edges(Changes::default())
    }
    fn edges(&mut self, mut changes: Changes) -> Changes {
        let mut held = [false; 7];
        for t in self.touches.values() {
            match t {
                Touch::Move { delta, .. } => {
                    held[0] |= delta[1] < -12.;
                    held[1] |= delta[1] > 12.;
                    held[2] |= delta[0] < -12.;
                    held[3] |= delta[0] > 12.;
                }
                Touch::Button(action) => {
                    if let Some(i) = HELD.iter().position(|a| a == action) {
                        held[i] = true;
                    }
                }
                _ => {}
            }
        }
        for (i, action) in HELD.iter().enumerate() {
            if held[i] != self.held[i] {
                changes.actions.push((*action, held[i]));
            }
        }
        self.held = held;
        changes
    }
    pub fn touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        p: [f64; 2],
        size: [f64; 2],
        hotbar_slot: Option<usize>,
    ) -> Changes {
        let mut out = Changes::default();
        match phase {
            TouchPhase::Started => {
                let [x, y] = [p[0] / size[0], p[1] / size[1]];
                let touch = if let Some(slot) = hotbar_slot {
                    // Select on contact. Dragging keeps this owner and leaves
                    // the original slot selected, even outside the bar.
                    out.actions.push((Action::SelectHotbar(slot), true));
                    Touch::Hotbar
                } else if x < 0.32 && y > 0.5 {
                    // One movement and one look owner; extra fingers cannot steal them.
                    if self
                        .touches
                        .values()
                        .any(|t| matches!(t, Touch::Move { .. }))
                    {
                        Touch::Tap
                    } else {
                        Touch::Move {
                            origin: p,
                            delta: [0.; 2],
                        }
                    }
                } else if x > 0.86 && y > 0.45 {
                    Touch::Button(if y < 0.63 {
                        Action::Jump
                    } else if y < 0.81 {
                        Action::Attack
                    } else {
                        Action::Use
                    })
                } else if y < 0.15 && x > 0.75 {
                    if x <= 0.9 {
                        out.actions
                            .extend([(Action::Inventory, true), (Action::Inventory, false)]);
                    }
                    Touch::Tap
                } else if y > 0.83 && x > 0.70 {
                    out.actions
                        .extend([(Action::Pick, true), (Action::Pick, false)]);
                    Touch::Tap
                } else if self.touches.values().any(|t| matches!(t, Touch::Look(_))) {
                    Touch::Tap
                } else {
                    Touch::Look(p)
                };
                self.touches.insert(id, touch);
            }
            TouchPhase::Moved => {
                if let Some(t) = self.touches.get_mut(&id) {
                    match t {
                        Touch::Move { origin, delta } => {
                            *delta = [p[0] - origin[0], p[1] - origin[1]]
                        }
                        Touch::Look(last) => {
                            out.look = [p[0] - last[0], p[1] - last[1]];
                            *last = p;
                        }
                        _ => {}
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.touches.remove(&id);
            }
        }
        self.edges(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client_lib::hud_layout::{self, GuiDraw, HotbarInputs, HotbarLayout};

    #[test]
    fn rendered_hotbar_owns_all_nine_slots_even_over_other_control_zones() {
        // With this HUD size the bar crosses both the movement and pick zones.
        let size = [320., 240.];
        let layout = HotbarLayout::new(320, 230);
        let draws = hud_layout::hotbar(
            320,
            230,
            &HotbarInputs {
                selected: 0,
                right_handed: true,
                offhand: false,
                slots: [Some(0.0); 9],
                offhand_pop: 0.0,
                attack: None,
            },
        );
        let mut controls = Controls::default();
        controls.touch(90, TouchPhase::Started, [40., 180.], size, None);
        assert_eq!(
            controls
                .touch(90, TouchPhase::Moved, [40., 150.], size, None)
                .actions,
            [(Action::Forward, true)]
        );
        let mut visited = Vec::new();
        for draw in draws {
            let GuiDraw::Item { slot, x, y, .. } = draw else {
                continue;
            };
            let p = [f64::from(x + 8), f64::from(y + 8)];
            let id = u64::from(slot);
            let changes = controls.touch(id, TouchPhase::Started, p, size, layout.slot_at(p));
            assert_eq!(
                changes.actions,
                [(Action::SelectHotbar(slot as usize), true)]
            );
            assert_eq!(changes.look, [0.; 2]);
            // Slide over the rest of the bar and through the attack/use zones.
            for point in [[240., p[1]], [315., 170.], [315., 210.], [160., 90.]] {
                let changes =
                    controls.touch(id, TouchPhase::Moved, point, size, layout.slot_at(point));
                assert!(changes.actions.is_empty());
                assert_eq!(changes.look, [0.; 2]);
            }
            assert!(
                controls
                    .touch(id, TouchPhase::Cancelled, p, size, None)
                    .actions
                    .is_empty()
            );
            visited.push(slot);
        }
        assert_eq!(visited, (0..9).collect::<Vec<_>>());

        controls.touch(91, TouchPhase::Started, [160., 90.], size, None);
        controls.touch(
            92,
            TouchPhase::Started,
            [160., 218.],
            size,
            layout.slot_at([160., 218.]),
        );
        assert_eq!(
            controls
                .touch(91, TouchPhase::Moved, [166., 100.], size, None)
                .look,
            [6., 10.]
        );
        // The host uses clear on menu transitions. Stale moves and releases
        // after returning to gameplay must be inert.
        assert_eq!(controls.clear().actions, [(Action::Forward, false)]);
        for phase in [TouchPhase::Moved, TouchPhase::Ended] {
            let changes = controls.touch(92, phase, [160., 90.], size, None);
            assert!(changes.actions.is_empty());
            assert_eq!(changes.look, [0.; 2]);
        }
    }

    #[test]
    fn moving_looking_and_actions_are_independent_and_cancel_together() {
        let mut c = Controls::default();
        let size = [1000., 500.];
        c.touch(1, TouchPhase::Started, [100., 400.], size, None);
        c.touch(1, TouchPhase::Moved, [100., 350.], size, None);
        c.touch(2, TouchPhase::Started, [500., 200.], size, None);
        c.touch(3, TouchPhase::Started, [950., 270.], size, None);
        c.touch(4, TouchPhase::Started, [950., 350.], size, None);
        c.touch(5, TouchPhase::Started, [950., 450.], size, None);
        let changes = c.touch(2, TouchPhase::Moved, [520., 190.], size, None);
        assert_eq!(changes.look, [20., -10.]);
        assert!(changes.actions.is_empty());
        assert_eq!(
            c.clear().actions,
            [
                (Action::Forward, false),
                (Action::Jump, false),
                (Action::Attack, false),
                (Action::Use, false)
            ]
        );
        // Resume, reconnect or finishing a portal never reclaims an old finger.
        for id in 1..=5 {
            let changes = c.touch(id, TouchPhase::Moved, [600., 300.], size, None);
            assert!(changes.actions.is_empty());
            assert_eq!(changes.look, [0., 0.]);
            assert!(
                c.touch(id, TouchPhase::Ended, [600., 300.], size, None)
                    .actions
                    .is_empty()
            );
        }
        assert_eq!(
            c.touch(6, TouchPhase::Started, [950., 270.], size, None)
                .actions,
            [(Action::Jump, true)]
        );
    }
    #[test]
    fn lifting_one_jump_finger_keeps_the_other_and_movement_held() {
        let mut c = Controls::default();
        let size = [1000., 500.];
        c.touch(1, TouchPhase::Started, [100., 400.], size, None);
        assert_eq!(
            c.touch(1, TouchPhase::Moved, [100., 350.], size, None)
                .actions,
            vec![(Action::Forward, true)]
        );
        c.touch(2, TouchPhase::Started, [950., 270.], size, None);
        c.touch(3, TouchPhase::Started, [950., 270.], size, None);
        assert!(
            c.touch(2, TouchPhase::Ended, [950., 270.], size, None)
                .actions
                .is_empty()
        );
        assert_eq!(
            c.touch(3, TouchPhase::Cancelled, [950., 270.], size, None)
                .actions,
            vec![(Action::Jump, false)]
        );
        assert_eq!(c.clear().actions, vec![(Action::Forward, false)]);
    }
    #[test]
    fn look_owner_does_not_jump_when_another_finger_starts_or_ends() {
        let mut c = Controls::default();
        let size = [1000., 500.];
        c.touch(1, TouchPhase::Started, [500., 200.], size, None);
        c.touch(2, TouchPhase::Started, [700., 200.], size, None);
        assert_eq!(
            c.touch(2, TouchPhase::Moved, [900., 200.], size, None).look,
            [0., 0.]
        );
        c.touch(2, TouchPhase::Ended, [900., 200.], size, None);
        assert_eq!(
            c.touch(1, TouchPhase::Moved, [510., 205.], size, None).look,
            [10., 5.]
        );
    }
    #[test]
    fn inventory_tap_has_one_edge_pair_and_releasing_other_touches_is_balanced() {
        let mut controls = Controls::default();
        let size = [1000., 500.];
        controls.touch(1, TouchPhase::Started, [100., 400.], size, None);
        controls.touch(1, TouchPhase::Moved, [100., 350.], size, None);
        assert_eq!(
            controls
                .touch(2, TouchPhase::Started, [820., 30.], size, None)
                .actions,
            [(Action::Inventory, true), (Action::Inventory, false)]
        );
        assert!(
            controls
                .touch(2, TouchPhase::Moved, [830., 30.], size, None)
                .actions
                .is_empty()
        );
        assert!(
            controls
                .touch(2, TouchPhase::Ended, [830., 30.], size, None)
                .actions
                .is_empty()
        );
        assert_eq!(controls.clear().actions, [(Action::Forward, false)]);
        assert!(
            controls
                .touch(1, TouchPhase::Ended, [100., 350.], size, None)
                .actions
                .is_empty()
        );
    }
}
