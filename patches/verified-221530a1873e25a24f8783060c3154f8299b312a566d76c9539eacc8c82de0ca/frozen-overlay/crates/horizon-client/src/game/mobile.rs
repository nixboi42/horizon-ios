//! UIKit adapter around the ordinary resource loader and network client.
//! The host owns the one winit loop/window and switches the sandbox cwd only
//! between game sessions. No mobile world, movement, chunk source or save format lives here.
use super::*;

#[path = "mobile_performance.rs"]
mod performance;

/// Quit the title screen, leaving the host loop running on iOS.
pub(super) fn quit_game(_event_loop: &dyn EventLoopCtl) {
    #[cfg(target_os = "ios")]
    {
        horizon_threads::ios_clipboard::return_to_launcher();
    }
    #[cfg(not(target_os = "ios"))]
    _event_loop.exit();
}

/// Default bindings, independent of a desktop user's physical key remapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Sneak,
    Sprint,
    Perspective,
    Attack,
    Use,
    Pick,
    Inventory,
    Pause,
    Chat,
    SelectHotbar(usize),
}

/// Touch input is separate from physical mappings and cannot release their holds.
#[cfg(target_os = "ios")]
pub(super) struct Input {
    pub(super) stick: [f32; 2],
    pub(super) modifiers: [bool; 2],
    pub(super) action_down: [bool; 2],
    pub(super) action_clicks: [bool; 2],
    pub(super) tap: Option<[f64; 3]>,
    pub(super) mine: Option<[f64; 2]>,
    pub(super) mine_started: bool,
    pub(super) ray: Option<[f64; 3]>,
    pub(super) crosshair: bool,
    pub(super) editing: bool,
}
#[cfg(target_os = "ios")]
impl Default for Input {
    fn default() -> Self {
        Self {
            stick: [0.; 2],
            modifiers: [false; 2],
            action_down: [false; 2],
            action_clicks: [false; 2],
            tap: None,
            mine: None,
            mine_started: false,
            ray: None,
            crosshair: true,
            editing: false,
        }
    }
}
#[cfg(target_os = "ios")]
impl Input {
    pub(super) fn clear_actions(&mut self) {
        let (crosshair, editing) = (self.crosshair, self.editing);
        *self = Self {
            crosshair,
            editing,
            ..Self::default()
        };
    }
}

/// One game session. Its resource ownership is the same as desktop App.
pub struct Session {
    app: App,
    suspended: bool,
    input_focus: Option<(u64, usize)>,
    input_epoch: u64,
    started: Instant,
    first_playable: bool,
    first_world_frame: bool,
    report_at: Instant,
    background_save: Option<std::sync::mpsc::Receiver<std::result::Result<(), String>>>,
}

/// Runs on the existing loader, before any dependent resource workers start.
pub(super) fn prepare_resources(root: &std::path::Path) -> Result<()> {
    let _step = startup::step("mobile.preflight");
    let started = Instant::now();
    if let Some(error) = missing_resources(root, None) {
        return Err(error.into());
    }
    horizon_world::datafix::set_resources(root);
    mods::register_components(root)?;
    mods::register_standalone_models();
    mods::register_layer_definitions();
    log::info!(
        "MOBILE preflight_ms={:.2}",
        started.elapsed().as_secs_f64() * 1000.
    );
    Ok(())
}

impl Session {
    /// The resource tree has already passed the native import integrity check.
    /// The game directory must be the app's isolated, writable sandbox directory.
    pub fn new(root: PathBuf, window: Arc<Window>, username: &str) -> Result<Self> {
        let started = Instant::now();
        let root = std::fs::canonicalize(root)?;
        install_game_hooks();
        #[cfg(not(target_os = "ios"))]
        prepare_resources(&root)?;
        // Parse only the adapter's arguments, never UIKit's process arguments or
        // a launcher session file. Local offline profiles retain the upstream path.
        let mut options = Options::parse(std::iter::empty())?.ok_or("missing options")?;
        options.resources = root.clone();
        options.player_name = Some(username.to_owned());
        if std::env::var("HORIZON_IOS_CONTROL").as_deref() == Ok("1") {
            options.control = Some(PathBuf::from("ctl.sock"));
        }
        // Constrained devices get a safe first launch; later launches respect the
        // player's own Video Settings instead of silently resetting them.
        let options_path = Game::options_path();
        let first_run = !options_path.exists();
        let mut settings = Settings::load(&options_path);
        // Play opens the title directly. Accessibility remains in the title menu.
        settings.file.set(
            "onboardAccessibility",
            horizon_client::options_file::V::Bool(false),
        );
        if first_run {
            options.distance_override = Some(2);
            options.distance = 2;
            settings.render_distance = 2;
            settings.simulation_distance = 5;
            settings.max_fps = 60;
        }
        let constrained = horizon_threads::process_memory::constrained_ios();
        // Per-instance migration: build 7 forcibly restored 30 FPS / 100% on
        // every Play. Apply the replacement defaults once, then honor sliders.
        let performance_marker = options_path.with_file_name("ios-performance-v1");
        let migrated = performance_marker.is_file();
        (settings.max_fps, settings.horizon.resolution_scale) = performance::defaults(
            settings.max_fps,
            settings.horizon.resolution_scale,
            first_run,
            constrained,
            migrated,
        );
        if !migrated {
            settings.save(&options_path);
            let saved = Settings::load(&options_path);
            if options_path.is_file()
                && saved.max_fps == settings.max_fps
                && saved.horizon.resolution_scale == settings.horizon.resolution_scale
                && let Err(error) = std::fs::write(&performance_marker, "1\n")
            {
                log::warn!("iOS performance defaults marker was not saved: {error}");
            }
        }
        if constrained {
            use horizon_client::graphics::{
                EntityCullingMode, FrameGeneration, GpuCullingMode, HdrMode,
            };
            settings.simulation_distance = 5;
            settings.mipmap_levels = settings.mipmap_levels.min(2);
            settings.vsync = true;
            let graphics = &mut settings.horizon;
            graphics.workers = 1;
            graphics.chunk_updates = 4;
            graphics.upload_budget_kib = 1024;
            graphics.texture_budget_mib = 32;
            graphics.lod_distance = 0;
            graphics.hdr = HdrMode::Off;
            graphics.hdr_smooth_gradients = false;
            graphics.taa = false;
            graphics.fxaa = false;
            graphics.dynamic_resolution = false;
            graphics.frame_generation = FrameGeneration::Off;
            graphics.gpu_culling = GpuCullingMode::Off;
            graphics.entity_culling = EntityCullingMode::Off;
            graphics.shader_pack.clear();
        }
        #[cfg(target_os = "ios")]
        horizon_threads::process_memory::report_footprint("play_requested");
        settings.horizon.api = horizon_client::graphics::GraphicsApi::Auto;
        // Runtime module loading is not a supported iOS facility.
        settings.mods.enabled.clear();
        // Initialize the surface before spawning an instance-bound loader.
        let renderer = if constrained {
            None
        } else {
            let boot = horizon_client::hud::HudAtlas::boot();
            let boot_started = Instant::now();
            let _boot_step = startup::step("mobile.renderer.boot");
            let renderer = loading_renderer(
                window.clone(),
                false,
                Some(&boot),
                None,
                None,
                settings.horizon.api,
            )?;
            drop(_boot_step);
            log::info!(
                "MOBILE boot_renderer_ms={:.2}",
                boot_started.elapsed().as_secs_f64() * 1000.
            );
            Some(renderer)
        };
        let mut loading = new_loading(options, settings, root, None, started)?;
        loading.renderer = renderer;
        loading.window = Some(window);
        log::info!("MOBILE gameplay resource loader started");
        Ok(Self {
            app: App::Loading(Box::new(loading)),
            suspended: false,
            input_focus: None,
            input_epoch: 0,
            started,
            first_playable: false,
            first_world_frame: false,
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

    pub fn loaded(&self) -> bool {
        matches!(&self.app, App::Ready(_))
    }

    /// UIKit can uncover the game window as soon as it has a title frame.
    pub fn title_visible(&self) -> bool {
        matches!(&self.app, App::Loading(loading) if loading.title_drawn) || self.loaded()
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
            Action::Sneak | Action::Sprint => {
                #[cfg(target_os = "ios")]
                {
                    let sprint = action == Action::Sprint;
                    let was_down =
                        std::mem::replace(&mut g.mobile_input.modifiers[usize::from(sprint)], down);
                    // Releasing Java's sprint key alone doesn't end an ongoing sprint.
                    if sprint
                        && was_down
                        && !down
                        && !g.keys.held(Mapping::Sprint)
                        && let Some(player) = &mut g.player
                        && player.sprinting
                    {
                        player.set_sprinting(false)?;
                    }
                }
                Ok(())
            }
            #[cfg(target_os = "ios")]
            Action::Attack | Action::Use => {
                let i = usize::from(action == Action::Use);
                if down && !g.mobile_input.action_down[i] {
                    g.mobile_input.action_clicks[i] = true;
                }
                g.mobile_input.action_down[i] = down;
                Ok(())
            }
            Action::SelectHotbar(slot) => {
                if down && g.touch_hotbar_visible() && slot < 9 {
                    // Same queue as the number keys. The client tick selects the
                    // slot and synchronizes it before any interaction uses it.
                    g.hotbar_presses.push(slot);
                }
                Ok(())
            }
            #[cfg(not(target_os = "ios"))]
            Action::Attack | Action::Use => g.mouse_mapping(
                if action == Action::Attack {
                    MouseButton::Left
                } else {
                    MouseButton::Right
                },
                down,
                Source::Agent,
            ),
            Action::Pick => g.mouse_mapping(MouseButton::Middle, down, Source::Agent),
            _ => {
                let key = match action {
                    Action::Forward => KeyCode::KeyW,
                    Action::Back => KeyCode::KeyS,
                    Action::Left => KeyCode::KeyA,
                    Action::Right => KeyCode::KeyD,
                    Action::Jump => KeyCode::Space,
                    Action::Inventory => KeyCode::KeyE,
                    Action::Perspective => KeyCode::F5,
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

    /// The dedicated UIKit button uses the normal chat action, including drafts.
    pub fn open_touch_chat(&mut self, el: &ActiveEventLoop) -> Result<bool> {
        if !self.playing() {
            return Ok(false);
        }
        self.press(el, KeyCode::KeyT)?;
        Ok(self.game().is_some_and(|g| g.screen == Screen::Chat))
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

    /// Native touch gestures bypass the desktop window input handler.
    pub fn note_touch_activity(&mut self) {
        if let Some(g) = self.game() {
            g.last_input = Instant::now();
        }
    }

    pub fn look(&mut self, delta: [f64; 2]) {
        if delta != [0.0; 2]
            && self.playing()
            && let Some(g) = self.game()
        {
            // Touch has its own gain and Y direction. It does not inherit the
            // mouse's inversion, smoothing or sensitivity curve.
            let gain = if g.scoping() { 0.22 } else { 1.8 };
            g.turn_player(delta[0] * gain, delta[1] * gain);
            g.last_input = Instant::now();
        }
    }

    pub fn touch_menu(&mut self) -> u8 {
        let Some(g) = self.game() else {
            return 0;
        };
        if g.screen_over_playing() {
            return 0;
        }
        match g.screen {
            Screen::Paused if g.pause_menu => 1,
            Screen::Options if g.ui.options.page() == horizon_client::options_ui::Page::Main => 2,
            _ => 0,
        }
    }

    pub fn touch_playing_hud(&mut self) -> bool {
        self.playing() && self.game().is_some_and(|g| !g.ui.hide_gui)
    }

    pub fn touch_flight(&mut self) -> bool {
        self.game()
            .and_then(|g| g.player.as_ref())
            .is_some_and(|p| p.flying)
    }

    pub fn touch_stick(&mut self, stick: [f32; 2]) {
        #[cfg(target_os = "ios")]
        if let Some(g) = self.game() {
            g.mobile_input.stick = stick;
        }
        #[cfg(not(target_os = "ios"))]
        let _ = stick;
    }

    pub fn touch_ui(&mut self, crosshair: bool, editing: bool) {
        #[cfg(target_os = "ios")]
        if let Some(g) = self.game() {
            g.mobile_input.crosshair = crosshair;
            g.mobile_input.editing = editing;
        }
        #[cfg(not(target_os = "ios"))]
        let _ = (crosshair, editing);
    }

    pub fn clear_touch(&mut self) {
        #[cfg(target_os = "ios")]
        if let Some(g) = self.game() {
            g.mobile_input.clear_actions();
        }
    }

    pub fn touch_target(&mut self, point: Option<[f64; 2]>, mining: bool) {
        #[cfg(target_os = "ios")]
        if let Some(g) = self.game() {
            if let Some([x, y]) = point {
                if mining {
                    g.mobile_input.mine = Some([x, y]);
                    g.mobile_input.mine_started = true;
                } else {
                    g.mobile_input.tap = g.touch_ray([x, y]);
                }
            } else {
                g.mobile_input.mine = None;
                g.mobile_input.mine_started = false;
            }
        }
        #[cfg(not(target_os = "ios"))]
        let _ = (point, mining);
    }

    /// Bounds reserved for vanilla's bar, including the offhand and status rows.
    pub fn touch_hotbar_rect(&mut self) -> Option<[f64; 4]> {
        let g = self.game()?;
        let renderer = g.renderer.as_ref()?;
        let size = [renderer.config.width, renderer.config.height];
        let hud = HudBatch::new(size);
        let bottom =
            hud_layout::HotbarLayout::safe_bottom(&hud, size[1], g.mobile_hud_bottom_inset);
        let scale = f64::from(hud.scale);
        Some([
            (hud.width / 2. - 122.) as f64 * scale,
            (bottom as f64 - 42.) * scale,
            244. * scale,
            44. * scale,
        ])
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

    /// Dispatch the ordinary menu press and identify the field that accepted it.
    #[cfg(target_os = "ios")]
    pub fn press_text_field(
        &mut self,
        el: &ActiveEventLoop,
        id: WindowId,
        event: WindowEvent,
    ) -> bool {
        let screen = self.game().map(|g| g.key_screen());
        let accepted =
            horizon_client::ui::track_text_field_press(|| self.window_event(el, id, event));
        accepted && self.game().map(|g| g.key_screen()) == screen
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
        let playable = self.playing();
        match &mut self.app {
            App::Loading(l) => {
                if let Some(error) = l.error.take() {
                    return Err(error);
                }
            }
            App::Ready(s) => {
                let g = &s.host;
                if g.world_frames > 0 && !self.first_world_frame {
                    self.first_world_frame = true;
                    startup::mark("mobile.first_world_frame");
                    log::info!(
                        "MOBILE first_world_frame_ms={:.2}",
                        self.started.elapsed().as_secs_f64() * 1000.
                    );
                }
                if playable && !self.first_playable {
                    self.first_playable = true;
                    startup::mark("mobile.first_playable");
                    #[cfg(target_os = "ios")]
                    if !horizon_threads::process_memory::constrained_ios() {
                        super::resource_load::finish_mobile_startup();
                    }
                    #[cfg(target_os = "ios")]
                    horizon_threads::process_memory::report_footprint("first_playable");
                    log::info!(
                        "MOBILE first_playable_ms={:.2} memory_transport={} integrated_server={}",
                        self.started.elapsed().as_secs_f64() * 1000.,
                        g.remote.is_some(),
                        g.integrated.is_some()
                    );
                }
                if self.report_at.elapsed() >= Duration::from_secs(5) {
                    self.report_at = Instant::now();
                    #[cfg(target_os = "ios")]
                    horizon_threads::process_memory::report_footprint(if playable {
                        "gameplay"
                    } else if g.integrated_starting.is_some() || g.remote.is_some() {
                        "world_loading"
                    } else {
                        "menu"
                    });
                    log::info!(
                        "MOBILE_CLIENT fps={:.2} effective_cap={} columns={} resident_bytes={:?} player={:?} ground={:?} mspt={:?}",
                        g.fps,
                        g.framerate_limit(),
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
        #[cfg(target_os = "ios")]
        if let Some(game) = self.game() {
            game.audio.suspend_output();
        }
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

    #[cfg(target_os = "ios")]
    pub fn memory_warning(&mut self) {
        if let Some(game) = self.game() {
            game.audio.trim_cache();
            // Requeue, rather than losing the only mesh of an unchanged section.
            for (key, _) in game.pending_uploads.drain(..) {
                game.world.dirty_sections.insert(key);
            }
            if let Some(renderer) = &mut game.renderer {
                renderer.relieve_memory_pressure();
            }
        } else if let App::Loading(loading) = &mut self.app
            && let Some(renderer) = &mut loading.renderer
        {
            renderer.relieve_memory_pressure();
        }
        horizon_threads::release_free_memory();
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

    /// Begin the ordinary staged world close without exiting winit. The owner
    /// polls this until the integrated server has saved and stopped, then drops
    /// the session on the UI thread.
    pub fn begin_return_to_launcher(&mut self) -> Result<bool> {
        if let App::Ready(split) = &mut self.app {
            let game = &mut split.host;
            if let Some(starting) = &game.integrated_starting {
                starting.handle.request_stop();
            }
            game.leave_server(None)?;
        }
        self.finish_return_to_launcher()
    }

    /// Keep the instance directory bound until the close and the resource
    /// workers that may still use it have completed.
    pub fn finish_return_to_launcher(&mut self) -> Result<bool> {
        match &mut self.app {
            App::Loading(loading) => {
                // Discard UI messages before checking the worker. Dropping
                // the title acknowledgement releases a worker waiting on UI
                // setup when we quit without ever showing the title.
                for message in loading.receiver.try_iter() {
                    if let LoaderMsg::Done(result) = message {
                        loading.loaded = Some(result);
                    }
                }
                if loading
                    .handle
                    .as_ref()
                    .is_some_and(|job| !job.is_finished())
                {
                    return Ok(false);
                }
                // The loader can publish a tail even if the normal handoff wasn't reached.
                let tail_running = |loaded: &Loaded| {
                    loaded
                        .rest_loader
                        .as_ref()
                        .is_some_and(|tail| !tail.handle.is_finished())
                };
                if loading
                    .loaded
                    .as_ref()
                    .is_some_and(|result| result.as_ref().is_ok_and(tail_running))
                {
                    return Ok(false);
                }
                // A finished worker may have just queued Done after the first drain.
                for message in loading.receiver.try_iter() {
                    if let LoaderMsg::Done(result) = message {
                        loading.loaded = Some(result);
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            App::Ready(split) => {
                let game = &mut split.host;
                if game.pending_save.is_some() {
                    game.poll_closing_save()?;
                }
                // pack reload's worker can write instance-local cache files.
                game.drive_pack_reload();
                Ok(game.pending_save.is_none()
                    && game.remote.is_none()
                    && game.integrated.is_none()
                    && game
                        .integrated_starting
                        .as_ref()
                        .is_none_or(|starting| starting.handle.stopped())
                    && game
                        .creation_job
                        .as_ref()
                        .is_none_or(JoinHandle::is_finished)
                    && game
                        .rest_loader
                        .as_ref()
                        .is_none_or(|tail| tail.handle.is_finished())
                    && game.pack_reload.is_none())
            }
        }
    }
}

impl Game<'_> {
    fn touch_hotbar_visible(&self) -> bool {
        !self.ui.hide_gui && !self.replay.playing() && !self.spectator()
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

#[path = "mobile_performance.rs"]
mod performance;
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

#[cfg(target_os = "ios")]
impl Game<'_> {
    pub(super) fn touch_mining_held(&self) -> bool {
        self.mobile_input.action_down[0] || self.mobile_input.mine.is_some()
    }

    pub(super) fn touch_ray(&self, [x, y]: [f64; 2]) -> Option<[f64; 3]> {
        let (_, yaw, pitch, fov) = self.locator_camera?;
        // The point is in units of half the drawable height, with Y up.
        let t = (fov.to_radians() / 2.).tan();
        let view = horizon_client::camera::view_matrix(yaw, pitch);
        Some(
            view.transpose()
                .transform_vector3(glam::Vec3::new(x as f32 * t, y as f32 * t, -1.))
                .normalize()
                .as_dvec3()
                .to_array(),
        )
    }
}
