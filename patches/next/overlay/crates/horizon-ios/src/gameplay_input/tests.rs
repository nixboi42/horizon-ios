use super::*;

fn fixture() -> (Settings, Layout) {
    let settings = Settings::default();
    let layout = Layout::resolve(
        &settings,
        Rect {
            x: 48.,
            y: 0.,
            w: 716.,
            h: 354.,
        },
        Rect {
            x: 280.,
            y: 297.,
            w: 255.,
            h: 57.,
        },
    );
    (settings, layout)
}
fn event(id: u64, phase: TouchPhase, point: [f64; 2], now: Instant) -> Event {
    Event {
        id,
        phase,
        point,
        now,
        slot: None,
        editor: false,
    }
}

#[test]
fn movement_look_and_two_jump_fingers_are_independent() {
    use TouchPhase::*;
    let (mut s, l) = fixture();
    let mut controls = Controls::default();
    let now = Instant::now();
    let [x, y] = l.buttons[MOVE].center();
    controls.touch(event(1, Started, [x, y], now), &l, &mut s);
    controls.touch(event(1, Moved, [x - 23., y - 23.], now), &l, &mut s);
    let [left, forward] = controls.stick;
    assert!(left > 0.3 && left < 0.7 && forward == left);
    let jump = l.buttons[JUMP].center();
    assert_eq!(
        controls
            .touch(event(2, Started, jump, now), &l, &mut s)
            .edges[0],
        Some((Action::Jump, true))
    );
    controls.touch(event(3, Started, jump, now), &l, &mut s);
    controls.touch(event(4, Started, [400., 200.], now), &l, &mut s);
    let out = controls.touch(event(4, Moved, [413., 178.], now), &l, &mut s);
    assert_eq!(out.look, [13., -22.]);
    assert_eq!(out.target, None);
    assert_eq!(
        controls.touch(event(2, Ended, jump, now), &l, &mut s).edges[0],
        None
    );
    assert_eq!(controls.clear().edges[0], Some((Action::Jump, false)));
    assert_eq!(controls.stick, [0., 0.]);
    assert_eq!(
        controls
            .touch(event(4, Ended, [413., 178.], now), &l, &mut s)
            .target,
        None
    );
}

#[test]
fn sprint_and_crouch_toggle_on_tap_and_clear_when_leaving_gameplay() {
    use TouchPhase::*;
    let (mut settings, layout) = fixture();
    let now = Instant::now();
    let mut controls = Controls::default();

    for (button, action, edge) in [(SPRINT, Action::Sprint, 2), (SNEAK, Action::Sneak, 1)] {
        let p = layout.buttons[button].center();
        assert_eq!(
            controls
                .touch(event(1, Started, p, now), &layout, &mut settings)
                .edges[edge],
            Some((action, true))
        );
        assert_eq!(
            controls
                .touch(event(1, Ended, p, now), &layout, &mut settings)
                .edges[edge],
            None
        );
        assert!(controls.down(button));

        assert_eq!(
            controls
                .touch(event(2, Started, p, now), &layout, &mut settings)
                .edges[edge],
            Some((action, false))
        );
        controls.touch(event(2, Ended, p, now), &layout, &mut settings);
        assert!(!controls.down(button));

        controls.touch(event(3, Started, p, now), &layout, &mut settings);
        assert_eq!(controls.clear().edges[edge], Some((action, false)));
        assert!(!controls.down(button));
        assert_eq!(
            controls
                .touch(event(3, Ended, p, now), &layout, &mut settings)
                .edges[edge],
            None
        );
    }
}

#[test]
fn tap_drag_hold_and_cancel_do_not_overlap() {
    use TouchPhase::*;
    let (mut s, _) = fixture();
    s.scheme = Scheme::JoystickTap;
    let (_, baseline) = fixture();
    let l = Layout::resolve(&s, baseline.safe, baseline.hotbar);
    let mut c = Controls::default();
    let t = Instant::now();
    let p = [400., 175.];
    let late = t + Duration::from_millis(400);
    c.touch(event(1, Started, p, t), &l, &mut s);
    assert_eq!(
        c.touch(event(1, Ended, p, t), &l, &mut s).target,
        Some(Target::Tap(p))
    );
    c.touch(event(2, Started, p, t), &l, &mut s);
    assert_eq!(c.advance(late, &s).target, Some(Target::StartMine(p)));
    let moved = [420., 175.];
    let out = c.touch(event(2, Moved, moved, late), &l, &mut s);
    assert_eq!(out.target, None);
    assert_eq!(out.look, [20., 0.]);
    assert_eq!(
        c.touch(event(2, Ended, moved, late), &l, &mut s).target,
        Some(Target::StopMine)
    );
    c.touch(event(3, Started, p, t), &l, &mut s);
    assert_eq!(c.touch(event(3, Cancelled, p, t), &l, &mut s).target, None);
    assert_eq!(c.advance(late, &s).target, None);
}

#[test]
fn hotbar_and_hud_editing_never_start_world_gestures() {
    use TouchPhase::*;
    let (mut s, l) = fixture();
    s.scheme = Scheme::JoystickTap;
    let mut c = Controls::default();
    let now = Instant::now();
    let mut slot = event(1, Started, l.hotbar.center(), now);
    slot.slot = Some(8);
    assert_eq!(
        c.touch(slot, &l, &mut s).pulse,
        Some(Action::SelectHotbar(8))
    );
    assert_eq!(
        c.touch(event(1, Ended, [400., 180.], now), &l, &mut s)
            .target,
        None
    );
    let point = l.buttons[USE].center();
    for (phase, point) in [
        (Started, point),
        (Moved, [400., 180.]),
        (Ended, [400., 180.]),
    ] {
        let out = c.touch(
            Event {
                editor: true,
                ..event(2, phase, point, now)
            },
            &l,
            &mut s,
        );
        assert_eq!(out.target, None);
        assert_eq!(out.look, [0.; 2]);
        assert_eq!(out.edges, [None; 5]);
        assert_eq!(out.pulse, None);
    }
    assert!(s.placed()[USE].center.is_some());
}

#[test]
fn crowded_saved_layouts_stay_accessible_on_small_and_large_devices() {
    let mut s = Settings::default();
    s.size = 1.4;
    s.left_handed = true;
    s.placed().fill(Placement {
        center: Some([0.5, 1.]),
        size: 1.4,
        opacity: 0.6,
    });
    assert_eq!(Settings::decode(&s.encode()), s);
    for (w, h) in [(692., 354.), (736., 414.), (1024., 748.)] {
        let safe = Rect {
            x: 47.,
            y: 6.,
            w,
            h,
        };
        let bar = Rect {
            x: w / 2. - 60.,
            y: h - 50.,
            w: 220.,
            h: 50.,
        };
        let l = Layout::resolve(&s, safe, bar);
        for (i, r) in l.buttons.iter().enumerate() {
            assert!(r.w >= 42.);
            assert_eq!(l.hit(r.center()), Some(i));
            assert!(safe.contains([r.x, r.y]));
            assert!(safe.contains([r.x + r.w, r.y + r.h]));
            assert!(!r.intersects(bar));
            assert!(!l.buttons[..i].iter().any(|b| r.intersects(*b)));
        }
    }
}
