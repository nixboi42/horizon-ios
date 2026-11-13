//! Real Game state and normal wire codecs; no renderer or external server needed.
use super::*;
use horizon_protocol::{
    connection::{Connection, Pipe},
    packets::{Empty, configuration as cfg, login, play},
};

fn session() -> Session {
    // Cargo runs test binaries in the package directory even when invoked
    // from a scratch cwd. Require an explicit disposable game directory.
    let game_dir = std::fs::canonicalize(
        std::env::var_os("HORIZON_TEST_GAME_DIR").expect("set HORIZON_TEST_GAME_DIR to mktemp -d"),
    )
    .unwrap();
    assert!(
        game_dir.starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap()),
        "session fixtures must run under the system temporary directory"
    );
    std::env::set_current_dir(game_dir).unwrap();
    let root =
        PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("prepared resources"));
    let settings = Settings::default();
    let loaded = load_resources(
        root.clone(),
        None,
        &root,
        &settings,
        Instant::now(),
        false,
        true,
        &Progress::new(),
        &mut |_, _| unreachable!("no menu handoff"),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::Client;
    // Session owns a process-lifetime Game just as the native host does.
    let res = Box::leak(Box::new(loaded.resources));
    let game = Game::new(
        res,
        loaded.pack,
        loaded.gui.unwrap(),
        options,
        settings,
        Audio::silent(&root),
        None,
    )
    .unwrap();
    Session {
        app: App::Ready(Box::new(split::Split::new(game))),
        suspended: false,
        input_focus: None,
        input_epoch: 0,
        started: Instant::now(),
        first_playable: false,
        report_at: Instant::now(),
        background_save: None,
    }
}

fn wait_for(s: &mut Session, check: impl Fn(&Game<'_>) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let g = s.game().unwrap();
        g.poll_remote().unwrap();
        if check(g) {
            return;
        }
        assert!(Instant::now() < deadline, "network fixture timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

const HOTBAR_ITEMS: [&str; 9] = [
    "minecraft:stone",
    "minecraft:dirt",
    "minecraft:granite",
    "minecraft:diorite",
    "minecraft:andesite",
    "minecraft:sand",
    "minecraft:gravel",
    "minecraft:oak_planks",
    "minecraft:glass",
];

fn fill_hotbar(s: &mut Session) {
    let g = s.game().unwrap();
    for (slot, item) in HOTBAR_ITEMS.iter().enumerate() {
        g.inventory[slot] = Some(horizon_client::items::ItemStack::new(*item, 1));
    }
}

fn tap_hotbar(s: &mut Session, slot: usize, size: [u32; 2], points: f64, display_scale: f64) {
    s.set_hud_safe_area(points, display_scale);
    let hud = HudBatch::new(size);
    let scale = f64::from(hud.scale);
    // Tap the centre of the vanilla item's 16x16 icon, 11 GUI units
    // above the bar's bottom. Derive the expected anchor independently.
    let safe_edge = f64::from(size[1]) - (points + 6.0) * display_scale;
    let bottom = if points > 0.0 {
        (safe_edge / scale).floor()
    } else {
        f64::from(hud.height)
    };
    let x = (f64::from(hud.width).div_euclid(2.0) - 80.0 + 20.0 * slot as f64) * scale;
    let position = [x, (bottom - 11.0) * scale];
    let g = s.game().unwrap();
    assert_eq!(g.touch_hotbar_slot(size, position), Some(slot));
    if points > 0.0 {
        assert_eq!(g.touch_hotbar_slot(size, [x, safe_edge + 1.0]), None);
    }
    s.action(Action::SelectHotbar(slot), true).unwrap();
}

#[test]
#[ignore = "needs HORIZON_TEST_RESOURCES and HORIZON_TEST_GAME_DIR; run with --test-threads=1"]
fn integrated_session_uses_the_same_visibility_rule() {
    let mut s = session();
    s.game()
        .unwrap()
        .open_integrated(
            "TouchFixture",
            Some("TouchFixture"),
            Some(1234),
            true,
            Some(&horizon_client::title::WorldOptions::default()),
        )
        .unwrap();
    assert!(!s.playing(), "server startup");
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let g = s.game().unwrap();
        g.poll_integrated_start().unwrap();
        g.poll_remote().unwrap();
        if g.player.is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "integrated player did not initialize"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let g = s.game().unwrap();
    assert!(g.integrated.is_some());
    g.world_frames = 1;
    // The headless fixture has no GPU to signal its player section.
    g.remote
        .as_mut()
        .unwrap()
        .level_load
        .loading_packets_received();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !s.playing() {
        let g = s.game().unwrap();
        g.poll_remote().unwrap();
        g.tick_remote().unwrap();
        assert!(
            Instant::now() < deadline,
            "integrated loading did not finish"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(s.playing());
    fill_hotbar(&mut s);
    tap_hotbar(&mut s, 8, [2340, 1080], 21.0, 3.0);
    let g = s.game().unwrap();
    g.tick_remote().unwrap();
    assert_eq!(g.held().unwrap().item.as_ref(), HOTBAR_ITEMS[8]);
    g.tick_remote().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let selected = g.integrated.as_ref().unwrap().call(|server| {
            server
                .lan
                .as_ref()
                .unwrap()
                .hub
                .guests
                .first()
                .map(|guest| guest.selected)
        });
        if selected == Some(Some(8)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "server did not apply carried slot"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    s.action(Action::Pause, true).unwrap();
    assert!(!s.playing());
    s.back().unwrap();
    assert!(s.playing());
    s.game().unwrap().leave_server(None).unwrap();
    assert!(!s.playing());
}

fn connect(s: &mut Session) -> Connection {
    let (client, server) = Pipe::pair();
    s.game()
        .unwrap()
        .connect_local(client, "TouchTest")
        .unwrap();
    // Memory transport only replaces the socket. Login, configuration, Game's
    // Remote and all encoded play packets are the production implementations.
    assert!(s.game().unwrap().integrated.is_none());
    let mut server = Connection::local(server);
    server
        .reader
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    assert!(!s.playing(), "connecting");
    server.recv().unwrap(); // handshake
    server.recv().unwrap(); // profile
    server
        .send(
            &login::Clientbound::LoginFinished(login::LoginFinished {
                profile: login::GameProfile {
                    uuid: 1,
                    name: "TouchTest".into(),
                    properties: vec![],
                },
                session: 1,
            })
            .encode(),
        )
        .unwrap();
    server.recv().unwrap(); // login acknowledged
    server.recv().unwrap(); // brand
    server.recv().unwrap(); // client information
    assert!(!s.playing(), "configuration");
    server
        .send(&cfg::Clientbound::FinishConfiguration(Empty).encode())
        .unwrap();
    server.recv().unwrap(); // configuration acknowledged
    wait_for(s, |g| g.remote.as_ref().unwrap().joined);
    s.game().unwrap().world_frames = 1;
    s.game()
        .unwrap()
        .remote
        .as_mut()
        .unwrap()
        .level_load
        .start_client_load(0, horizon_client::level_loading::Reason::Other);
    assert!(
        !s.playing(),
        "a rendered world without a player is not controllable"
    );
    server
        .send(
            &play::Clientbound::PlayerPosition(play::PlayerPosition {
                teleport: 7,
                position: [0.5, 70., 0.5],
                delta: [0.; 3],
                yaw: 0.,
                pitch: 0.,
                relatives: 0,
            })
            .encode(),
        )
        .unwrap();
    wait_for(s, |g| g.player.is_some());
    assert!(!s.playing(), "player exists but terrain is still loading");
    let g = s.game().unwrap();
    g.remote.as_mut().unwrap().logged_in = true;
    g.streaming
        .insert_remote(&mut g.world, horizon_client::chunk::Chunk::empty(0, 0));
    g.remote
        .as_mut()
        .unwrap()
        .level_load
        .loading_packets_received();
    g.tick_remote().unwrap();
    assert!(
        s.playing(),
        "delayed remote player and loaded terrain enable controls"
    );
    server
}

#[test]
#[ignore = "needs HORIZON_TEST_RESOURCES and HORIZON_TEST_GAME_DIR; run with --test-threads=1"]
fn session_visibility_and_remote_packet_routing() {
    let mut s = session();
    assert!(!s.playing(), "startup/title");
    let mut server = connect(&mut s);
    server
        .reader
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    fill_hotbar(&mut s);

    // Both landscape orientations keep a centred bar. Also exercise a
    // drawable resize and a zero-bottom-inset iPad 5.
    let previous_scale = horizon_client::hud::GUI_SCALE.load(Ordering::Relaxed);
    for (index, (size, inset, display_scale, gui_scale)) in [
        ([2340, 1080], 21., 3., 0),
        ([2340, 1080], 21., 3., 1),
        ([2048, 1536], 0., 2., 2),
    ]
    .into_iter()
    .enumerate()
    {
        horizon_client::hud::GUI_SCALE.store(gui_scale, Ordering::Relaxed);
        for slot in (1..=9).map(|i| (i + index) % 9) {
            tap_hotbar(&mut s, slot, size, inset, display_scale);
            let g = s.game().unwrap();
            g.tick_remote().unwrap();
            assert_eq!(g.held().unwrap().item.as_ref(), HOTBAR_ITEMS[slot]);
            g.tick_remote().unwrap();
            loop {
                let (id, body) = server.recv().unwrap();
                if let play::Serverbound::SetCarriedItem(p) =
                    play::Serverbound::decode(id, &body).unwrap()
                {
                    if p.slot == 0 && index == 0 && slot == 1 {
                        continue; // Initial synchronization at login.
                    }
                    assert_eq!(usize::try_from(p.slot).unwrap(), slot);
                    break;
                }
            }
        }
    }
    horizon_client::hud::GUI_SCALE.store(previous_scale, Ordering::Relaxed);

    // Visibility alone is insufficient: run the actual client tick and decode
    // what the server receives, rather than asserting calls to mocked send().
    s.action(Action::Forward, true).unwrap();
    s.action(Action::Jump, true).unwrap();
    s.action(Action::Attack, true).unwrap();
    s.game().unwrap().tick_remote().unwrap();
    let mut movement = false;
    let mut attack = false;
    while !(movement && attack) {
        let (id, body) = server.recv().unwrap();
        let packet = play::Serverbound::decode(id, &body).unwrap();
        eprintln!("touch fixture received: {packet:?}");
        match packet {
            play::Serverbound::PlayerInput(p) => {
                assert_eq!(p.flags & 17, 17);
                movement = true;
            }
            play::Serverbound::Punch(_) => attack = true,
            _ => {}
        }
    }
    for screen in [
        Screen::Paused,
        Screen::Options,
        Screen::Inventory,
        Screen::Chat,
    ] {
        s.game().unwrap().screen = screen;
        assert!(!s.playing());
        s.action(Action::Forward, false).unwrap();
        s.action(Action::Jump, false).unwrap();
        s.action(Action::Attack, false).unwrap();
        s.action(Action::Forward, true).unwrap();
        s.action(Action::Attack, true).unwrap();
        s.action(Action::SelectHotbar(3), true).unwrap();
        let g = s.game().unwrap();
        assert!(g.hotbar_presses.is_empty());
        assert!(!g.keys.raw(&KeyCode::KeyW));
        assert!(!g.keys.raw(&KeyCode::Space));
        assert!(!g.left);
        g.screen = Screen::Playing;
        assert!(s.playing(), "resume/close chat");
    }
    s.game().unwrap().tick_remote().unwrap();
    loop {
        let (id, body) = server.recv().unwrap();
        if let play::Serverbound::PlayerInput(p) = play::Serverbound::decode(id, &body).unwrap() {
            assert_eq!(p.flags, 0, "releasing touches reaches the server too");
            break;
        }
    }
    tap_hotbar(&mut s, 4, [2340, 1080], 21.0, 3.0);
    s.action(Action::Use, true).unwrap();
    s.game().unwrap().tick_remote().unwrap();
    let mut carried = false;
    loop {
        let (id, body) = server.recv().unwrap();
        match play::Serverbound::decode(id, &body).unwrap() {
            play::Serverbound::SetCarriedItem(p) => {
                assert_eq!(p.slot, 4);
                carried = true;
            }
            play::Serverbound::UseItem(p) => {
                assert!(carried, "server must hear the selection before item use");
                assert_eq!(p.hand, 0);
                break;
            }
            _ => {}
        }
    }
    s.action(Action::Use, false).unwrap();
    // Every portal screen must suspend controls, even when it keeps the player.
    for reason in [
        horizon_client::level_loading::Reason::Other,
        horizon_client::level_loading::Reason::NetherPortal,
        horizon_client::level_loading::Reason::EndPortal,
    ] {
        s.game()
            .unwrap()
            .remote
            .as_mut()
            .unwrap()
            .level_load
            .start_client_load(0, reason);
        assert!(!s.playing());
        s.game()
            .unwrap()
            .remote
            .as_mut()
            .unwrap()
            .level_load
            .finish();
        assert!(s.playing());
    }
    s.action(Action::Forward, true).unwrap();
    s.action(Action::Use, true).unwrap();
    s.suspend();
    assert!(!s.playing());
    assert!(s.game().unwrap().keys.is_empty());
    assert!(!s.game().unwrap().right);
    s.resume().unwrap();
    assert!(!s.playing(), "resume remains paused");
    s.back().unwrap();
    assert!(s.playing());
    assert!(s.game().unwrap().keys.is_empty());

    s.action(Action::Forward, true).unwrap();
    s.action(Action::Attack, true).unwrap();
    s.game().unwrap().leave_server(None).unwrap();
    assert!(!s.playing());
    assert!(s.game().unwrap().keys.is_empty());
    assert!(!s.game().unwrap().left);
    drop(server);
    let _reconnected = connect(&mut s);
    assert!(s.playing());
    assert!(s.game().unwrap().keys.is_empty());
    s.game().unwrap().leave_server(None).unwrap();
}
