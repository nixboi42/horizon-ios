//! UIKit adapter around the ordinary resource loader and network client.
//! The host owns the one winit loop/window and establishes the sandbox cwd before
//! any workers start. No mobile world, movement, chunk source or save format lives here.
use super::*;

/// Default bindings, independent of a desktop user's physical key remapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Attack,
    Use,
    Pick,
    Inventory,
    Pause,
    Chat,
    SelectHotbar(usize),
}

/// One process-lifetime game. Its resource ownership is the same as desktop App.
pub struct Session {
    app: App,
    suspended: bool,
    input_focus: Option<(u64, usize)>,
    input_epoch: u64,
    started: Instant,
    first_playable: bool,
    report_at: Instant,
    background_save: Option<std::sync::mpsc::Receiver<std::result::Result<(), String>>>,
}

impl Session {
    /// The resource tree has already passed the native import integrity check.
    /// The game directory must be the app's isolated, writable sandbox directory.
    pub fn new(root: PathBuf, window: Arc<Window>) -> Result<Self> {
        let started = Instant::now();
        let root = std::fs::canonicalize(root)?;
        if let Some(error) = missing_resources(&root) {
            return Err(error.into());
        }
        install_game_hooks();
        mods::register_components(&root)?;
        mods::register_standalone_models();
        mods::register_layer_definitions();
        Behaviours::set_cache_dir(std::env::current_dir()?.join(".horizon-cache"));
        // Parse only the adapter's arguments, never UIKit's process arguments or
        // a launcher session file. Local offline profiles retain the upstream path.
        let mut options = Options::parse(std::iter::empty())?.ok_or("missing options")?;
        options.resources = root.clone();
        options.world = Some("Mobile Creative".into());
        if std::env::var("HORIZON_IOS_CONTROL").as_deref() == Ok("1") {
            options.control = Some(PathBuf::from("ctl.sock"));
        }
        options.seed = 1234;
        options.creative = true;
        // Constrained devices get a safe first launch; later launches respect the
        // player's own Video Settings instead of silently resetting them.
        let options_path = Game::options_path();
        let first_run = !options_path.exists();
        let mut settings = Settings::load(&options_path);
        if first_run {
            options.distance_override = Some(2);
            options.distance = 2;
            settings.render_distance = 2;
            settings.simulation_distance = 5;
            settings.max_fps = 60;
        }
        settings.horizon.api = horizon_client::graphics::GraphicsApi::Auto;
        // Runtime module loading is not a supported iOS facility.
        settings.mods.enabled.clear();
        let mut loading = new_loading(options, settings, root, None, started)?;
        loading.renderer = Some(loading_renderer(
            window.clone(),
            false,
            Some(&loading.boot),
            None,
            None,
            loading.settings.horizon.api,
        )?);
        loading.window = Some(window);
        log::info!("MOBILE gameplay resource loader started");
        Ok(Self {
            app: App::Loading(Box::new(loading)),
            suspended: false,
            input_focus: None,
            input_epoch: 0,
            started,
            first_playable: false,
            report_at: started,
            background_save: None,
        })
    }

    fn game(&mut self) -> Option<&mut Game<'static>> {
        match &mut self.app {
            App::Ready(split) => Some(&mut split.host),
            App::Loading(_) => None,
        }
    }

    pub fn playing(&self) -> bool {
        !self.suspended
            && matches!(&self.app, App::Ready(split) if split.host.title.is_none()
            && split.host.screen == Screen::Playing && !split.host.screen_over_playing()
            // Unlike remote_loading (the panorama/blur predicate), this also
            // covers portal loading screens while a player still exists.
            && !split.host.remote_screen_up()
            && split.host.pending_save.is_none()
            && split.host.remote.is_some() && split.host.player.is_some()
            && split.host.world_frames > 0)
    }

    pub fn action(&mut self, action: Action, down: bool) -> Result<()> {
        if action == Action::Pause {
            return if down { self.back() } else { Ok(()) };
        }
        // A queued press can arrive after a screen or connection changed.
        // Releases must always drain the normal mappings, even behind a menu.
        if down && !self.playing() {
            return Ok(());
        }
        let Some(g) = self.game() else { return Ok(()) };
        match action {
            Action::SelectHotbar(slot) => {
                if down && g.touch_hotbar_visible() && slot < 9 {
                    // Same queue as the number keys. The client tick selects the
                    // slot and synchronizes it before any interaction uses it.
                    g.hotbar_presses.push(slot);
                }
                Ok(())
            }
            Action::Attack | Action::Use | Action::Pick => g.mouse_mapping(
                match action {
                    Action::Attack => MouseButton::Left,
                    Action::Use => MouseButton::Right,
                    _ => MouseButton::Middle,
                },
                down,
                Source::Agent,
            ),
            _ => {
                let key = match action {
                    Action::Forward => KeyCode::KeyW,
                    Action::Back => KeyCode::KeyS,
                    Action::Left => KeyCode::KeyA,
                    Action::Right => KeyCode::KeyD,
                    Action::Jump => KeyCode::Space,
                    Action::Inventory => KeyCode::KeyE,
                    Action::Pause => KeyCode::Escape,
                    Action::Chat => KeyCode::KeyT,
                    _ => unreachable!(),
                };
                g.mapped_key(key, down, false, key, Source::Agent)
            }
        }
    }

    /// Reuse the screen's Escape action without leaving a held key behind.
    pub fn back(&mut self) -> Result<()> {
        let Some(g) = self.game() else { return Ok(()) };
        let result = if !g.prompt_open() && g.book.is_some() {
            // Book signing's Escape returns to the page; page Escape saves.
            g.book_input(Some(KeyCode::Escape), None);
            Ok(())
        } else if !g.prompt_open() && g.screen == Screen::BlockEditor {
            g.block_editor_key_code(KeyCode::Escape, None, None);
            Ok(())
        } else {
            g.pad_back()
        };
        g.keys.release(&KeyCode::Escape, Source::Agent);
        result
    }

    pub fn press(&mut self, el: &ActiveEventLoop, key: KeyCode) -> Result<()> {
        if key == KeyCode::Escape {
            return self.back();
        }
        let Some(g) = self.game() else { return Ok(()) };
        if key == KeyCode::Enter {
            if g.title.is_some() && g.screen != Screen::Options && !g.prompt_open() {
                let size = g.gui_size();
                let (action, sound) = g.title.as_mut().unwrap().confirm_key_size(size);
                g.save_title_options();
                if sound {
                    g.audio.play_button_click();
                }
                g.title_action(el, action)?;
            } else {
                g.pad_osk_output(horizon_client::osk::Output::Enter);
            }
            g.keys.release(&key, Source::Agent);
            return Ok(());
        }
        let result = g.mapped_key(key, true, false, key, Source::Agent);
        g.mapped_key(key, false, false, key, Source::Agent)?;
        result
    }

    pub fn input_state(&mut self) -> serde_json::Value {
        let Some(g) = self.game() else {
            return serde_json::Value::Null;
        };
        // UIKit owns text on iOS; never open the controller OSK on top of it.
        g.pad.menu.osk = None;
        let screen = g.key_screen();
        let address = g
            .title
            .as_mut()
            .and_then(|t| t.focused_field())
            .is_some_and(|(id, _, _)| {
                matches!(id.0, "server.address" | "server.direct" | "friend.relay")
            });
        let field = g.mobile_text_field(None);
        let focus = field.as_ref().map(|f| (screen, f.identity));
        if focus != self.input_focus {
            self.input_epoch = self.input_epoch.wrapping_add(1);
            self.input_focus = focus;
        }
        let epoch = self.input_epoch;
        let field = field.map(|f| {
            let cursor = scalar_to_utf16(&f.value, f.cursor);
            let anchor = scalar_to_utf16(&f.value, f.highlight);
            let multiline = f.multiline();
            serde_json::json!({"id": format!("{screen}:{}:{epoch}", f.identity), "value": f.value,
                "cursor": cursor, "anchor": anchor, "multiline": multiline, "address": address})
        });
        serde_json::json!({"ready": !self.suspended, "playing": self.playing(), "field": field})
    }

    pub fn edit_text(&mut self, id: &str, value: &str, cursor: usize, anchor: usize) {
        let epoch = self.input_epoch;
        let focus = self.input_focus;
        let Some(g) = self.game() else { return };
        let screen = g.key_screen();
        let Some(mut field) = g.mobile_text_field(None) else {
            return;
        };
        if focus != Some((screen, field.identity))
            || id != format!("{screen}:{}:{epoch}", field.identity)
        {
            return;
        }
        field.value = value.to_owned();
        field.cursor = utf16_to_scalar(value, cursor);
        field.highlight = utf16_to_scalar(value, anchor);
        g.mobile_text_field(Some(&field));
    }

    pub fn look(&mut self, delta: [f64; 2]) {
        if delta != [0.0; 2]
            && self.playing()
            && let Some(g) = self.game()
        {
            // MouseHandler's normal accumulated look path, including options.
            g.mouse.on_move(0., 0., delta[0], delta[1], true, true);
            g.last_input = Instant::now();
        }
    }

    pub fn set_hud_safe_area(&mut self, bottom_points: f64, display_scale: f64) {
        if let Some(g) = self.game() {
            // Reserve an extra six points for the hotbar and its off-hand frame.
            g.mobile_hud_bottom_inset = if bottom_points > 0.0 {
                (bottom_points + 6.0) * display_scale
            } else {
                0.0
            };
        }
    }

    pub fn hotbar_slot_at(&mut self, drawable_position: [f64; 2]) -> Option<usize> {
        if !self.playing() {
            return None;
        }
        let g = self.game()?;
        let r = g.renderer.as_ref()?;
        g.touch_hotbar_slot([r.config.width, r.config.height], drawable_position)
    }

    pub fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.window_event(el, id, event);
    }

    pub fn update(&mut self, el: &ActiveEventLoop) -> Result<()> {
        if let Some(receiver) = &self.background_save {
            match receiver.try_recv() {
                Ok(result) => {
                    self.background_save = None;
                    log::info!("MOBILE background save/flush completed: {result:?}");
                    result.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.background_save = None;
                    return Err("background save server disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.suspended {
            return Ok(());
        }
        self.app.about_to_wait(el);
        match &mut self.app {
            App::Loading(l) => {
                if let Some(error) = l.error.take() {
                    return Err(error);
                }
            }
            App::Ready(s) => {
                let g = &s.host;
                if g.world_frames > 0 && !self.first_playable {
                    self.first_playable = true;
                    log::info!(
                        "MOBILE first_playable_ms={:.2} memory_transport={} integrated_server={}",
                        self.started.elapsed().as_secs_f64() * 1000.,
                        g.remote.is_some(),
                        g.integrated.is_some()
                    );
                }
                if self.report_at.elapsed() >= Duration::from_secs(5) {
                    self.report_at = Instant::now();
                    log::info!(
                        "MOBILE_CLIENT fps={:.2} columns={} resident_bytes={:?} player={:?} ground={:?} mspt={:?}",
                        g.fps,
                        g.world.chunks.len(),
                        horizon_threads::process_memory::resident_bytes(),
                        g.player.as_ref().map(|p| p.position),
                        g.player.as_ref().map(|p| p.ground.on_ground),
                        g.integrated.as_ref().map(|s| s.smoothed_tick_ms())
                    );
                }
                if let Some(error) = s.host.error.take() {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// IntegratedServer saves on its transition to paused, on the server thread.
    /// Suspension must never synchronously join a busy server on the UIKit thread.
    pub fn suspend(&mut self) {
        self.suspended = true;
        match &mut self.app {
            App::Loading(l) => {
                if let Some(r) = &mut l.renderer {
                    r.suspend_surface();
                }
            }
            App::Ready(s) => {
                let g = &mut s.host;
                g.keys.clear();
                g.left = false;
                g.right = false;
                g.capture(false);
                if g.title.is_none() && g.screen == Screen::Playing && !g.screen_over_playing() {
                    g.screen = Screen::Paused;
                    g.pause_menu = true;
                }
                if let Some(server) = g
                    .integrated
                    .as_ref()
                    .or_else(|| g.integrated_starting.as_ref().map(|s| &s.handle))
                {
                    server.set_paused(true);
                    let (sender, receiver) = std::sync::mpsc::channel();
                    server.post(move |g| {
                        let started = Instant::now();
                        let result = g
                            .save_game()
                            .and_then(|()| g.save.flush())
                            .map_err(|e| e.to_string());
                        log::info!(
                            "MOBILE background save_ms={:.2}",
                            started.elapsed().as_secs_f64() * 1000.
                        );
                        let _ = sender.send(result);
                    });
                    self.background_save = Some(receiver);
                }
                if let Some(r) = &mut g.renderer {
                    r.suspend_surface();
                }
            }
        }
    }

    pub fn saving_in_background(&self) -> bool {
        self.background_save.is_some()
    }

    pub fn resume(&mut self) -> Result<()> {
        self.suspended = false;
        let renderer = match &mut self.app {
            App::Loading(l) => &mut l.renderer,
            App::Ready(s) => {
                s.host.previous = Instant::now();
                &mut s.host.renderer
            }
        };
        if let Some(r) = renderer {
            r.recreate_surface()?;
        }
        Ok(())
    }

    pub fn exiting(&mut self, el: &ActiveEventLoop) {
        self.app.exiting(el);
    }
}

impl Game<'_> {
    fn touch_hotbar_visible(&self) -> bool {
        !self.ui.hide_gui && self.studio_replay.is_none() && !self.spectator()
    }

    fn touch_hotbar_slot(&self, size: [u32; 2], position: [f64; 2]) -> Option<usize> {
        if !self.touch_hotbar_visible() {
            return None;
        }
        let hud = HudBatch::new(size);
        let bottom =
            hud_layout::HotbarLayout::safe_bottom(&hud, size[1], self.mobile_hud_bottom_inset);
        hud_layout::HotbarLayout::new(hud.width as i32, bottom)
            .slot_at(position.map(|px| px / f64::from(hud.scale)))
    }
}

fn scalar_to_utf16(text: &str, offset: usize) -> usize {
    text.chars().take(offset).map(char::len_utf16).sum()
}
fn utf16_to_scalar(text: &str, offset: usize) -> usize {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= offset
        })
        .count()
}
#[cfg(test)]
#[path = "mobile_tests.rs"]
mod session_tests;
#[cfg(test)]
mod mobile_text_tests {
    use super::*;
    #[test]
    fn selection_offsets_handle_turkish_emoji_and_composition() {
        let text = "İıŞğ😀e\u{301}";
        assert_eq!(scalar_to_utf16(text, 5), 6);
        assert_eq!(utf16_to_scalar(text, 5), 4);
        assert_eq!(utf16_to_scalar(text, 6), 5);
        for n in 0..=text.chars().count() {
            assert_eq!(utf16_to_scalar(text, scalar_to_utf16(text, n)), n);
        }
    }
}
