//! Native gameplay host with explicitly selected terrain/synthetic diagnostics.
//! winit owns UIKit; this host imports resources and forwards lifecycle/input to Game.
mod flight;
mod gameplay_input;
mod instances;
mod native_input;
mod resources;
mod scene;
mod terrain;

use std::sync::atomic::{AtomicU8, Ordering};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

struct NativeLog;
impl log::Log for NativeLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}
static NATIVE_LOG: NativeLog = NativeLog;
const TESTFLIGHT: bool = option_env!("HORIZON_TESTFLIGHT").is_some();
static APP_STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Monotonic elapsed time from the native host entry, shared with Swift metrics.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_startup_elapsed_ms() -> f64 {
    APP_STARTED
        .get()
        .map_or(0., |at| at.elapsed().as_secs_f64() * 1000.)
}

const COMMAND_NONE: u8 = 0;
const COMMAND_RETURN: u8 = 2;
static LAUNCHER_COMMAND: AtomicU8 = AtomicU8::new(COMMAND_NONE);
#[derive(Debug)]
enum HostEvent {
    LauncherAction,
    TouchControls(u32, f64),
    Play {
        username: String,
        game_dir: std::path::PathBuf,
    },
    ResourcesInstalled(std::path::PathBuf),
}
static EVENT_PROXY: std::sync::OnceLock<EventLoopProxy<HostEvent>> = std::sync::OnceLock::new();
static INSTANCE_STORE: std::sync::OnceLock<instances::Store> = std::sync::OnceLock::new();
static RESOURCE_STORE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
// A Play reservation covers launch, gameplay, and the complete session teardown.
static SESSION_OPEN: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
static RESOURCE_IO: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn c_text(raw: *const std::ffi::c_char) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: callers of the C ABI provide a live NUL-terminated string.
    unsafe { std::ffi::CStr::from_ptr(raw) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn valid_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// JSON requests from the launcher are serialized and may only run between sessions.
///
/// # Safety
/// The request is a live NUL-terminated string. Free the result with horizon_ios_free_string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_instances(
    request: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let result = (|| -> Result<_> {
        let open = SESSION_OPEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *open {
            return Err("a game session is open".into());
        }
        let request = c_text(request).ok_or("invalid instance request")?;
        let store = INSTANCE_STORE.get().ok_or("missing instance store")?;
        Ok(store.request(&serde_json::from_str(&request)?)?)
    })();
    let output = match result {
        Ok(catalog) => catalog.to_string(),
        Err(error) => {
            eprintln!("Horizon instance operation failed: {error}");
            "null".to_owned()
        }
    };
    std::ffi::CString::new(output).unwrap().into_raw()
}

/// # Safety
/// The pointer must come from horizon_ios_instances and be freed only once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_free_string(raw: *mut std::ffi::c_char) {
    if !raw.is_null() {
        // SAFETY: ownership is returned by the launcher.
        drop(unsafe { std::ffi::CString::from_raw(raw) });
    }
}

/// # Safety
/// Arguments must be live NUL-terminated strings for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_play(
    username: *const std::ffi::c_char,
    id: *const std::ffi::c_char,
) -> i32 {
    let result = (|| -> Result<()> {
        let mut open = SESSION_OPEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *open {
            return Err("a game session is open".into());
        }
        let username = c_text(username)
            .filter(|name| valid_username(name))
            .unwrap_or_else(|| "Player".into());
        let id = c_text(id).ok_or("missing instance ID")?;
        let game_dir = INSTANCE_STORE
            .get()
            .ok_or("missing instance store")?
            .game_dir(&id)?;
        let proxy = EVENT_PROXY.get().ok_or("missing event loop")?;
        proxy
            .send_event(HostEvent::Play { username, game_dir })
            .map_err(|_| "event loop closed")?;
        *open = true;
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("Horizon Play request failed: {error}");
        return 0;
    }
    1
}

/// # Safety
/// The folder must be a live NUL-terminated absolute path. The native caller
/// keeps any security-scoped access and file coordination live for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_import_resources(folder: *const std::ffi::c_char) -> i32 {
    let result = (|| -> std::io::Result<()> {
        use std::io::Error;
        let open = SESSION_OPEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *open {
            return Err(Error::other("a game session is open"));
        }
        let proxy = EVENT_PROXY
            .get()
            .ok_or_else(|| Error::other("missing event loop"))?;
        let folder =
            std::path::PathBuf::from(c_text(folder).ok_or_else(|| Error::other("missing path"))?);
        if !folder.is_absolute() {
            return Err(Error::other("resource source must be absolute"));
        }
        let storage = RESOURCE_STORE
            .get()
            .ok_or_else(|| Error::other("missing resource store"))?;
        let _import = RESOURCE_IO
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let root = resources::install(&folder, storage)?;
        proxy
            .send_event(HostEvent::ResourcesInstalled(root))
            .map_err(|_| Error::other("event loop closed"))
    })();
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Horizon resource import failed: {error}");
            if error.kind() == std::io::ErrorKind::Unsupported {
                1
            } else {
                2
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_minecraft_version() -> *const std::ffi::c_char {
    static VERSION: std::sync::OnceLock<std::ffi::CString> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| std::ffi::CString::new(horizon_core::VANILLA_VERSION).unwrap())
        .as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_return_to_launcher() {
    LAUNCHER_COMMAND.store(COMMAND_RETURN, Ordering::Release);
    if let Some(proxy) = EVENT_PROXY.get() {
        let _ = proxy.send_event(HostEvent::LauncherAction);
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// winit 0.30 defines UIKit inner_size as the safe-area rectangle. Metal
/// fills the entire window; UIKit positions the diagnostic label in its safe area.
fn drawable_size(window: &Window) -> winit::dpi::PhysicalSize<u32> {
    #[cfg(target_os = "ios")]
    {
        window.outer_size()
    }
    #[cfg(not(target_os = "ios"))]
    {
        window.inner_size()
    }
}

fn touch_safe_area(window: &Window) -> [f64; 4] {
    #[cfg(target_os = "ios")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        unsafe extern "C" {
            fn horizon_ios_touch_insets(view: *mut std::ffi::c_void, out: *mut f64);
        }
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::UiKit(handle) = handle.as_raw()
        {
            let mut insets = [0.; 4];
            // UIKit and both pointers are live on this event-loop thread.
            unsafe { horizon_ios_touch_insets(handle.ui_view.as_ptr(), insets.as_mut_ptr()) };
            return insets;
        }
    }
    let _ = window;
    [0.; 4]
}

fn safe_area_bottom_points(window: &Window) -> f64 {
    #[cfg(target_os = "ios")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        unsafe extern "C" {
            fn horizon_ios_safe_area_bottom(view: *mut std::ffi::c_void) -> f64;
        }
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::UiKit(handle) = handle.as_raw()
        {
            // SAFETY: called on the event loop's main thread with its live UIView.
            return unsafe { horizon_ios_safe_area_bottom(handle.ui_view.as_ptr()) };
        }
    }
    #[cfg(not(target_os = "ios"))]
    let _ = window;
    0.0
}

struct Gpu {
    instance: wgpu::Instance,
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    count: u32,
    uniform: wgpu::Buffer,
    bindings: wgpu::BindGroup,
}

impl Gpu {
    async fn new(window: Arc<Window>) -> Result<Self> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::METAL;
        let instance = wgpu::Instance::new(desc);
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await?;
        eprintln!("Horizon iOS adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Horizon iOS diagnostic"),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        let size = drawable_size(&window);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("Metal surface has no configuration")?;
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);
        let depth = Self::depth(&device, &config);
        let mesh = scene::diagnostic_mesh()?;
        let count = mesh.len() as u32;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Horizon baked model"),
            contents: bytemuck::cast_slice(&mesh),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Diagnostic only"),
            source: wgpu::ShaderSource::Wgsl(include_str!("diagnostic.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Horizon model diagnostic"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        eprintln!(
            "Horizon asset baker: {} vertices; surface {}x{} {:?}",
            count, config.width, config.height, config.format
        );
        Ok(Self {
            instance,
            surface: Some(surface),
            device,
            queue,
            config,
            depth,
            pipeline,
            vertices,
            count,
            uniform,
            bindings,
        })
    }

    fn depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Depth"),
                size: wgpu::Extent3d {
                    width: config.width,
                    height: config.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    fn resize(&mut self, window: &Window) {
        let size = drawable_size(window);
        if size.width == 0 || size.height == 0 {
            return;
        }
        let max = self.device.limits().max_texture_dimension_2d;
        self.config.width = size.width.min(max);
        self.config.height = size.height.min(max);
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.config);
        }
        self.depth = Self::depth(&self.device, &self.config);
        eprintln!(
            "Drawable {}x{}, scale {}",
            size.width,
            size.height,
            window.scale_factor()
        );
    }

    fn restore(&mut self, window: Arc<Window>) -> Result<()> {
        if self.surface.is_none() {
            self.surface = Some(self.instance.create_surface(window.clone())?);
        }
        self.resize(&window);
        Ok(())
    }

    fn draw(&mut self, window: &Window, yaw: f32, pitch: f32) -> Result<bool> {
        if drawable_size(window).width == 0 || drawable_size(window).height == 0 {
            return Ok(false);
        }
        let Some(surface) = &self.surface else {
            return Ok(false);
        };
        let (frame, reconfigure) = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => (f, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => (f, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.resize(window);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = None;
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Metal surface validation failed".into());
            }
        };
        let projection = glam::Mat4::perspective_rh(
            55f32.to_radians(),
            self.config.width as f32 / self.config.height as f32,
            0.1,
            100.0,
        );
        let eye = glam::Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        ) * 3.2;
        let matrix = projection * glam::Mat4::look_at_rh(eye, glam::Vec3::ZERO, glam::Vec3::Y);
        self.queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Diagnostic frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.045,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bindings, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..self.count, 0..1);
        }
        window.pre_present_notify();
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        if reconfigure {
            self.resize(window);
        }
        Ok(true)
    }
}

#[derive(Clone, Copy, Debug)]
enum Initialization {
    Uninitialized,
    LoadingResources,
    PreparingWorld,
    UploadingGeometry,
    Ready,
    Failed,
}

struct App {
    initialization: Initialization,
    launched: Instant,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    terrain: Option<Box<terrain::Terrain>>,
    gameplay: Option<Box<horizon_client::game::mobile::Session>>,
    controls: gameplay_input::Controls,
    touch_settings: gameplay_input::layout::Settings,
    touch_draft: Option<gameplay_input::layout::Settings>,
    touch_mode: u32,
    touch_layout: gameplay_input::layout::Layout,
    touch_geometry: Option<(gameplay_input::layout::Rect, gameplay_input::layout::Rect)>,
    touch_frame: Option<gameplay_input::ui::Frame>,
    menu_touch: Option<u64>,
    #[cfg(target_os = "ios")]
    text_tap: Option<([f64; 2], String)>,
    gameplay_ready: bool,
    gameplay_title_visible: bool,
    gameplay_stopping: bool,
    session_failed: bool,
    session_started: bool,
    resource_root: Option<std::path::PathBuf>,
    resource_status: &'static str,
    import: Option<std::thread::JoinHandle<std::io::Result<std::path::PathBuf>>>,
    terrain_attempted: bool,
    preparation: Option<std::thread::JoinHandle<std::result::Result<terrain::Prepared, String>>>,
    prepared: Option<terrain::Prepared>,
    flight: flight::Flight,
    movement_time: Instant,
    active: bool,
    next_frame: Instant,
    last_frame: Option<Instant>,
    samples: Vec<f64>,
    cpu_samples: Vec<f64>,
    sampled_peak_bytes: u64,
    yaw: f32,
    pitch: f32,
}
impl Default for App {
    fn default() -> Self {
        Self {
            initialization: Initialization::Uninitialized,
            launched: Instant::now(),
            window: None,
            gpu: None,
            terrain: None,
            gameplay: None,
            controls: Default::default(),
            touch_settings: Self::load_touch_settings(),
            touch_draft: None,
            touch_mode: 0,
            touch_layout: Default::default(),
            touch_geometry: None,
            touch_frame: None,
            menu_touch: None,
            #[cfg(target_os = "ios")]
            text_tap: None,
            gameplay_ready: false,
            gameplay_title_visible: false,
            gameplay_stopping: false,
            session_failed: false,
            session_started: false,
            resource_root: None,
            resource_status: "launcher.loading",
            import: None,
            terrain_attempted: false,
            preparation: None,
            prepared: None,
            flight: flight::Flight::default(),
            movement_time: Instant::now(),
            active: false,
            next_frame: Instant::now(),
            last_frame: None,
            samples: Vec::with_capacity(300),
            cpu_samples: Vec::with_capacity(300),
            sampled_peak_bytes: 0,
            yaw: 0.65,
            pitch: 0.4,
        }
    }
}
impl App {
    fn release_instance(&self) {
        if let Some(store) = INSTANCE_STORE.get() {
            if let Err(error) = std::env::set_current_dir(&store.parent) {
                eprintln!("Horizon could not reset its working directory: {error}");
            }
        }
        *SESSION_OPEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
    }

    fn finish_session(&mut self) {
        self.touch_mode = 0;
        self.touch_draft = None;
        self.controls.selected = None;
        self.release_gameplay_controls();
        self.draw_touch_frame(0);
        drop(self.gameplay.take());
        horizon_threads::release_free_memory();
        self.gameplay_stopping = false;
        self.gameplay_ready = false;
        self.gameplay_title_visible = false;
        background_save(false);
        native_input::reset();
        self.release_instance();
        status(if std::mem::take(&mut self.session_failed) {
            "launcher.error"
        } else {
            self.resource_status
        });
    }

    fn touch_settings_path() -> Option<std::path::PathBuf> {
        Some(INSTANCE_STORE.get()?.parent.join("touch-controls.json"))
    }
    fn load_touch_settings() -> gameplay_input::layout::Settings {
        Self::touch_settings_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .map_or_else(Default::default, |data| {
                gameplay_input::layout::Settings::decode(&data)
            })
    }
    fn save_touch_settings(&self) -> std::io::Result<()> {
        if let Some(path) = Self::touch_settings_path() {
            let scratch = path.with_extension("tmp");
            std::fs::write(&scratch, self.touch_settings.encode().to_string())?;
            std::fs::rename(scratch, path)?;
        }
        Ok(())
    }
    fn touch_command(&mut self, command: u32, value: f64) {
        let menu = self.gameplay.as_mut().map_or(0, |g| g.touch_menu());
        match command {
            30 | 31 if self.gameplay_ready => {
                use horizon_client::game::mobile::Action;
                let pulse = Some(if command == 30 {
                    Action::Chat
                } else {
                    Action::Pause
                });
                self.apply_gameplay_controls(gameplay_input::Changes {
                    pulse,
                    ..Default::default()
                });
                self.sync_gameplay_controls();
                self.sync_touch_ui();
                return;
            }
            0 if self.active && menu == 2 && self.touch_mode == 0 => self.touch_mode = 3,
            1 if self.active && menu == 1 && self.touch_mode == 0 => self.touch_mode = 4,
            2 if self.touch_mode != 0 => {
                if let Some(draft) = self.touch_draft.take() {
                    let previous = std::mem::replace(&mut self.touch_settings, draft);
                    if let Err(error) = self.save_touch_settings() {
                        self.touch_draft =
                            Some(std::mem::replace(&mut self.touch_settings, previous));
                        log::warn!("Could not save touch controls: {error}");
                        #[cfg(target_os = "ios")]
                        {
                            unsafe extern "C" {
                                fn horizon_ios_touch_save_failed();
                            }
                            unsafe { horizon_ios_touch_save_failed() };
                        }
                        return;
                    }
                }
                self.touch_mode = 0;
            }
            3 if self.touch_mode != 0 => {
                self.touch_draft = None;
                self.touch_mode = 0;
            }
            10..=18 if self.touch_mode == 3 => {
                if let Some(draft) = &mut self.touch_draft {
                    draft.set((command - 10) as usize, value);
                }
            }
            20..=22 if self.touch_mode == 4 => {
                if let Some(draft) = &mut self.touch_draft
                    && let Some(i) = self.controls.selected
                {
                    let p = &mut draft.placed()[i];
                    match command {
                        20 => p.size = value.clamp(0.75, 1.4),
                        21 => p.opacity = value.clamp(0.4, 1.),
                        _ => *p = Default::default(),
                    }
                }
            }
            23 if self.touch_mode == 4 => {
                if let Some(draft) = &mut self.touch_draft {
                    draft.placed().fill(Default::default());
                }
            }
            _ => return,
        }
        if command <= 3 {
            self.release_gameplay_controls();
            self.controls.selected = None;
            if matches!(command, 0 | 1) {
                self.touch_draft = Some(self.touch_settings.clone());
            }
        }
        self.touch_geometry = None;
        self.sync_touch_ui();
    }
    fn touch_settings(&self) -> &gameplay_input::layout::Settings {
        self.touch_draft.as_ref().unwrap_or(&self.touch_settings)
    }
    fn draw_touch_frame(&mut self, mode: u32) {
        let flying =
            matches!(mode, 1 | 4) && self.gameplay.as_mut().is_some_and(|g| g.touch_flight());
        let frame = gameplay_input::ui::Frame::new(
            mode,
            &self.touch_layout,
            &self.controls,
            self.touch_settings(),
            flying,
        );
        if self.touch_frame.as_ref() != Some(&frame) {
            frame.send();
            self.touch_frame = Some(frame);
        }
    }
    fn sync_touch_ui(&mut self) {
        let Some(game) = &mut self.gameplay else {
            return;
        };
        let menu = if self.active { game.touch_menu() } else { 0 };
        if self.active
            && ((self.touch_mode == 3 && menu != 2) || (self.touch_mode == 4 && menu != 1))
        {
            self.touch_mode = 0;
            self.touch_draft = None;
            self.controls.selected = None;
            self.release_gameplay_controls();
            self.touch_geometry = None;
        }
        let game = self.gameplay.as_mut().unwrap();
        let mode = if !self.active {
            0
        } else if self.touch_mode != 0 {
            self.touch_mode
        } else if self.gameplay_ready && game.touch_playing_hud() {
            1
        } else if menu != 0 {
            if menu == 1 { 2 } else { 5 }
        } else {
            0
        };
        let crosshair = self.touch_settings().scheme == gameplay_input::layout::Scheme::Crosshair;
        self.gameplay
            .as_mut()
            .unwrap()
            .touch_ui(crosshair, self.touch_mode == 4);
        if matches!(mode, 1 | 4) {
            self.refresh_touch_layout();
        }
        self.draw_touch_frame(mode);
    }
    fn refresh_touch_layout(&mut self) {
        use gameplay_input::layout::{Layout, Rect};
        let Some(w) = &self.window else { return };
        let scale = w.scale_factor();
        let size = drawable_size(w);
        let [top, left, bottom, right] = touch_safe_area(w);
        let safe = Rect {
            x: left,
            y: top,
            w: (size.width as f64 / scale - left - right).max(0.),
            h: (size.height as f64 / scale - top - bottom).max(0.),
        };
        let game = self.gameplay.as_mut().unwrap();
        game.set_hud_safe_area(bottom, scale);
        let bar = game
            .touch_hotbar_rect()
            .map_or_else(Rect::default, |[x, y, w, h]| Rect {
                x: x / scale,
                y: y / scale,
                w: w / scale,
                h: h / scale,
            });
        if self.touch_geometry != Some((safe, bar)) {
            self.touch_layout = Layout::resolve(self.touch_settings(), safe, bar);
            self.touch_geometry = Some((safe, bar));
        }
    }

    fn apply_gameplay_controls(&mut self, changes: gameplay_input::Changes) {
        use gameplay_input::Target;
        use horizon_client::game::mobile::Action;
        if let Some(game) = &mut self.gameplay {
            for (action, down) in changes.edges.into_iter().flatten() {
                if let Err(error) = game.action(action, down) {
                    eprintln!("MOBILE input failed: {error}");
                }
            }
            game.touch_stick(self.controls.stick);
            game.look(changes.look);
            if let Some(action) = changes.pulse {
                if action == Action::Chat {
                    native_input::horizon_ios_chat();
                } else if let Err(error) = game
                    .action(action, true)
                    .and_then(|()| game.action(action, false))
                {
                    eprintln!("MOBILE input failed: {error}");
                }
            }
            let project = |p: [f64; 2]| {
                let window = self.window.as_ref().unwrap();
                let size = drawable_size(window);
                let scale = window.scale_factor();
                let h = f64::from(size.height).max(1.);
                [
                    (2. * p[0] * scale - f64::from(size.width)) / h,
                    1. - 2. * p[1] * scale / h,
                ]
            };
            match changes.target {
                Some(Target::Tap(p)) => game.touch_target(Some(project(p)), false),
                Some(Target::StartMine(p)) => game.touch_target(Some(project(p)), true),
                Some(Target::StopMine) => game.touch_target(None, false),
                None => {}
            }
        }
    }
    fn release_gameplay_controls(&mut self) {
        let changes = self.controls.clear();
        self.apply_gameplay_controls(changes);
        if let Some(game) = &mut self.gameplay {
            game.clear_touch();
        }
        self.menu_touch = None;
        #[cfg(target_os = "ios")]
        {
            self.text_tap = None;
        }
    }
    fn sync_gameplay_controls(&mut self) {
        let playing = self.active
            && self.touch_mode == 0
            && !native_input::owns_text()
            && self.gameplay.as_ref().is_some_and(|game| game.playing());
        if playing != self.gameplay_ready {
            self.gameplay_ready = playing;
            self.release_gameplay_controls();
            status(if playing {
                "gameplay.ready"
            } else {
                "gameplay.menu"
            });
        }
    }
    fn gameplay_touch(&mut self, el: &ActiveEventLoop, id: WindowId, t: winit::event::Touch) {
        use winit::event::{DeviceId, ElementState, MouseButton, TouchPhase};
        if !self.active {
            return;
        }
        self.sync_gameplay_controls();
        if self.touch_mode == 3 {
            return;
        }
        let editor = self.touch_mode == 4;
        if self.gameplay_ready || editor {
            if !editor && t.phase != TouchPhase::Cancelled {
                self.gameplay.as_mut().unwrap().note_touch_activity();
            }
            self.refresh_touch_layout();
            let w = self.window.as_ref().unwrap();
            let scale = w.scale_factor();
            let position = [t.location.x, t.location.y];
            let slot = if !editor && t.phase == TouchPhase::Started {
                self.gameplay.as_mut().unwrap().hotbar_slot_at(position)
            } else {
                None
            };
            let event = gameplay_input::Event {
                id: t.id,
                phase: t.phase,
                point: position.map(|px| px / scale),
                slot,
                now: Instant::now(),
                editor,
            };
            let settings = self
                .touch_draft
                .as_mut()
                .unwrap_or(&mut self.touch_settings);
            let changes = self.controls.touch(event, &self.touch_layout, settings);
            if changes.layout_changed {
                self.touch_geometry = None;
            }
            if !editor {
                self.apply_gameplay_controls(changes);
                self.sync_gameplay_controls();
            }
            self.sync_touch_ui();
        } else {
            #[cfg(target_os = "ios")]
            let w = self.window.as_ref().unwrap();
            // One pointer owns a menu click. Other fingers never release it.
            if t.phase == TouchPhase::Started && self.menu_touch.is_none() {
                self.menu_touch = Some(t.id);
            }
            if self.menu_touch != Some(t.id) {
                return;
            }
            let game = self.gameplay.as_mut().unwrap();
            game.window_event(
                el,
                id,
                WindowEvent::CursorMoved {
                    device_id: DeviceId::dummy(),
                    position: t.location,
                },
            );
            if t.phase != TouchPhase::Moved {
                let event = WindowEvent::MouseInput {
                    device_id: DeviceId::dummy(),
                    state: if t.phase == TouchPhase::Started {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    },
                    button: MouseButton::Left,
                };
                #[cfg(target_os = "ios")]
                if t.phase == TouchPhase::Started {
                    self.text_tap = game
                        .press_text_field(el, id, event)
                        .then(|| native_input::focused_field(game))
                        .flatten()
                        .map(|field| ([t.location.x, t.location.y], field));
                } else {
                    game.window_event(el, id, event);
                }
                #[cfg(not(target_os = "ios"))]
                game.window_event(el, id, event);
            }
            #[cfg(target_os = "ios")]
            if let Some(([x, y], _)) = &self.text_tap
                && (t.location.x - x).hypot(t.location.y - y) > 10.0 * w.scale_factor()
            {
                self.text_tap = None;
            }
            if matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.menu_touch = None;
                #[cfg(target_os = "ios")]
                if let Some((_, field)) = self.text_tap.take()
                    && t.phase == TouchPhase::Ended
                {
                    native_input::sync(game, self.active, el, Some(field));
                }
            }
        }
    }
    fn transition(&mut self, state: Initialization) {
        self.initialization = state;
        if self.window.is_some() {
            status(match state {
                Initialization::Uninitialized | Initialization::LoadingResources => {
                    "terrain.loading"
                }
                Initialization::PreparingWorld => "terrain.preparing",
                Initialization::UploadingGeometry => "terrain.uploading",
                Initialization::Ready if self.gameplay.is_some() => {
                    if self.gameplay_ready {
                        "gameplay.ready"
                    } else {
                        "gameplay.menu"
                    }
                }
                Initialization::Ready => "terrain.ready",
                Initialization::Failed => "diagnostic.failed",
            });
        }
        eprintln!(
            "STARTUP state={state:?} launch_ms={:.2}",
            self.launched.elapsed().as_secs_f64() * 1000.
        );
    }
    fn fail(&mut self, error: impl std::fmt::Display) {
        self.transition(Initialization::Failed);
        eprintln!("Horizon iOS error: {error}");
        status("diagnostic.failed");
        self.active = false;
    }
    fn sample(&mut self, cpu_seconds: f64) {
        self.cpu_samples.push(cpu_seconds);
        self.sampled_peak_bytes = self
            .sampled_peak_bytes
            .max(horizon_threads::process_memory::resident_bytes().unwrap_or(0));
        let now = Instant::now();
        if let Some(last) = self.last_frame {
            self.samples.push(now.duration_since(last).as_secs_f64());
        }
        self.last_frame = Some(now);
        if self.samples.len() == 300 {
            let mean = self.samples.iter().sum::<f64>() / 300.;
            self.samples.sort_by(f64::total_cmp);
            let low = 3. / self.samples[297..].iter().sum::<f64>();
            eprintln!(
                "{} frames=300 fps={:.2} low_1pct={:.2} p50_ms={:.2} p95_ms={:.2} p99_ms={:.2} max_ms={:.2} resident_bytes={:?}",
                if self.terrain.is_some() {
                    "TERRAIN"
                } else {
                    "SYNTHETIC"
                },
                1. / mean,
                low,
                self.samples[149] * 1000.,
                self.samples[284] * 1000.,
                self.samples[296] * 1000.,
                self.samples[299] * 1000.,
                horizon_threads::process_memory::resident_bytes()
            );
            for (kind, nanos, count) in horizon_client_lib::streaming::job_stats::take() {
                if count > 0 {
                    eprintln!(
                        "STREAM_WORK kind={kind} count={count} total_ms={:.2}",
                        nanos as f64 / 1e6
                    );
                }
            }
            if let Some(t) = &self.terrain {
                eprintln!(
                    "TERRAIN drawn_sections={} terrain_draw_calls={} gpu_pass_medians_ms={:?}",
                    t.renderer.drawn_sections,
                    t.renderer.section_draw_calls,
                    t.renderer.own_pass_times()
                );
            }
            self.cpu_samples.sort_by(f64::total_cmp);
            let count = self.cpu_samples.len();
            let (sections, triangles) = self
                .terrain
                .as_ref()
                .map_or((0, 12), |t| (t.sections, t.triangles));
            eprintln!(
                "FRAME_WORK cpu_mean_ms={:.2} cpu_p99_ms={:.2} cpu_max_ms={:.2} sampled_peak_bytes={} chunks={} sections={sections} triangles={triangles}",
                self.cpu_samples.iter().sum::<f64>() / count as f64 * 1000.,
                self.cpu_samples[(count - 1) * 99 / 100] * 1000.,
                self.cpu_samples[count - 1] * 1000.,
                self.sampled_peak_bytes,
                self.terrain.as_ref().map_or(0, |t| t.chunks)
            );
            self.samples.clear();
            self.cpu_samples.clear();
        }
    }
}
impl ApplicationHandler<HostEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.active = true;
        self.movement_time = Instant::now();
        self.next_frame = Instant::now();
        self.last_frame = None;
        self.samples.clear();
        self.cpu_samples.clear();
        if self.window.is_none() {
            match event_loop.create_window(Window::default_attributes().with_title("Horizon iOS")) {
                Ok(w) => {
                    #[cfg(target_os = "ios")]
                    {
                        use winit::platform::ios::{ValidOrientations, WindowExtIOS};
                        unsafe extern "C" {
                            fn horizon_ios_bind_view(view: *mut std::ffi::c_void);
                        }
                        // SAFETY: winit just created its window on the main thread.
                        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                        if let Ok(handle) = w.window_handle()
                            && let RawWindowHandle::UiKit(handle) = handle.as_raw()
                        {
                            // SAFETY: the UIView belongs to this live winit window.
                            unsafe {
                                horizon_ios_bind_view(handle.ui_view.as_ptr());
                            }
                        }
                        w.set_valid_orientations(ValidOrientations::Landscape);
                        w.set_prefers_status_bar_hidden(true);
                        if let Some(monitor) = w.current_monitor() {
                            w.set_scale_factor(monitor.scale_factor());
                        }
                    }
                    self.window = Some(Arc::new(w));
                }
                Err(e) => {
                    self.fail(e);
                    return;
                }
            }
        }
        if let Some(game) = &mut self.gameplay {
            if !self.gameplay_stopping
                && let Err(error) = game.resume()
            {
                eprintln!("Horizon iOS gameplay could not resume: {error}");
                self.session_failed = true;
                LAUNCHER_COMMAND.store(COMMAND_RETURN, Ordering::Release);
            }
            return;
        }
        if TESTFLIGHT
            || !matches!(
                std::env::var("HORIZON_IOS_SCENE").as_deref(),
                Ok("terrain" | "synthetic")
            )
        {
            status(self.resource_status);
            return;
        }
        let window = self.window.as_ref().unwrap().clone();
        if let Some(prepared) = self.prepared.take() {
            self.transition(Initialization::UploadingGeometry);
            self.flight.clear();
            self.gpu = None;
            match pollster::block_on(terrain::Terrain::new(window.clone(), prepared)) {
                Ok(t) => {
                    self.flight.position = t.start;
                    self.terrain = Some(Box::new(t));
                    self.last_frame = None;
                    self.samples.clear();
                    self.cpu_samples.clear();
                }
                Err(e) => {
                    self.transition(Initialization::Failed);
                    eprintln!("TERRAIN GPU initialization failed: {e}");
                }
            }
            self.movement_time = Instant::now();
        }
        let result = if let Some(t) = &mut self.terrain {
            t.renderer.recreate_surface()
        } else {
            match &mut self.gpu {
                Some(g) => g.restore(window),
                None => pollster::block_on(Gpu::new(window)).map(|g| self.gpu = Some(g)),
            }
        };
        if let Err(e) = result {
            self.fail(e);
        } else {
            status(if matches!(self.initialization, Initialization::Failed) {
                "diagnostic.failed"
            } else if matches!(self.initialization, Initialization::PreparingWorld) {
                "terrain.preparing"
            } else if matches!(self.initialization, Initialization::UploadingGeometry) {
                "terrain.uploading"
            } else if self.terrain.is_some() {
                "terrain.ready"
            } else if self.import.is_some() || self.preparation.is_some() {
                "terrain.loading"
            } else if self.resource_root.is_some() {
                "diagnostic.ready"
            } else {
                "diagnostic.missing_resources"
            });
            eprintln!("Horizon iOS resumed");
        }
    }
    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.active = false;
        self.release_gameplay_controls();
        self.draw_touch_frame(0);
        if let Some(game) = &mut self.gameplay {
            background_save(true);
            game.suspend();
            if !game.saving_in_background() {
                background_save(false);
            }
        }
        self.flight.clear();
        self.last_frame = None;
        if let Some(t) = &mut self.terrain {
            t.renderer.suspend_surface();
        }
        if let Some(g) = &mut self.gpu {
            g.surface = None;
        }
        eprintln!("Horizon iOS suspended; surface released");
    }
    fn memory_warning(&mut self, _: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        {
            horizon_threads::process_memory::report_footprint("memory_warning");
            if let Some(game) = &mut self.gameplay {
                game.memory_warning();
            } else {
                horizon_threads::release_free_memory();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.gameplay.is_some() {
            if self.gameplay_stopping {
                return;
            }
            if matches!(event, WindowEvent::CloseRequested) {
                LAUNCHER_COMMAND.store(COMMAND_RETURN, Ordering::Release);
                return;
            }
            // UIKit owns typing, but releases must still clear physical keys
            // held before the field opened (otherwise movement can stick).
            let native_text_event = matches!(event, WindowEvent::Ime(_))
                || matches!(&event, WindowEvent::KeyboardInput { event, .. }
                    if event.state == winit::event::ElementState::Pressed);
            if native_text_event && native_input::owns_text() {
                return;
            }
            if let WindowEvent::Touch(touch) = event {
                if touch.phase == winit::event::TouchPhase::Started
                    && !self.gameplay_ready
                    && self.touch_mode == 0
                    && self.menu_touch.is_none()
                {
                    native_input::finish_composition();
                    // Apply committed edits before the game can select a different field.
                    native_input::sync(
                        self.gameplay.as_mut().unwrap(),
                        self.active,
                        event_loop,
                        None,
                    );
                }
                self.gameplay_touch(event_loop, id, touch);
            } else {
                if matches!(event, WindowEvent::Focused(false) | WindowEvent::Resized(_)) {
                    self.release_gameplay_controls();
                }
                if self.touch_mode != 0
                    && matches!(
                        event,
                        WindowEvent::KeyboardInput { .. }
                            | WindowEvent::MouseInput { .. }
                            | WindowEvent::CursorMoved { .. }
                            | WindowEvent::MouseWheel { .. }
                    )
                {
                    return;
                }
                if self.active {
                    // UIKit's inner_size is its safe area; Metal and touch
                    // coordinates cover the full surface.
                    let event = match event {
                        WindowEvent::Resized(_) => {
                            WindowEvent::Resized(drawable_size(self.window.as_ref().unwrap()))
                        }
                        other => other,
                    };
                    let game = self.gameplay.as_mut().unwrap();
                    if matches!(
                        event,
                        WindowEvent::Resized(_) | WindowEvent::RedrawRequested
                    ) {
                        let window = self.window.as_ref().unwrap();
                        game.set_hud_safe_area(
                            safe_area_bottom_points(window),
                            window.scale_factor(),
                        );
                    }
                    game.window_event(event_loop, id, event);
                }
            }
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(false) => self.flight.clear(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.flight.clear();
                if let (Some(t), Some(w)) = (&mut self.terrain, &self.window) {
                    let size = drawable_size(w);
                    if size.width > 0 && size.height > 0 {
                        t.renderer.resize(size.width, size.height);
                    }
                }
                if let (Some(g), Some(w)) = (&mut self.gpu, &self.window) {
                    g.resize(w);
                }
            }
            WindowEvent::Touch(t) => {
                if let Some(w) = &self.window {
                    let scale = w.scale_factor();
                    let size = drawable_size(w);
                    self.flight.sensitivity =
                        f32::from_bits(SENSITIVITY.load(std::sync::atomic::Ordering::Relaxed));
                    self.flight.touch(
                        t.id,
                        t.phase,
                        [t.location.x, t.location.y],
                        scale,
                        [
                            size.width as f32 / scale as f32,
                            size.height as f32 / scale as f32,
                        ],
                    );
                    // Keep the original model orbit available for graphics diagnosis.
                    self.yaw = self.flight.yaw.to_radians() + 0.65;
                    self.pitch = (-self.flight.pitch.to_radians() + 0.65).clamp(-1.3, 1.3);
                }
            }
            WindowEvent::RedrawRequested if self.active => {
                let frame_start = Instant::now();
                self.flight
                    .step(frame_start.duration_since(self.movement_time).as_secs_f64());
                self.movement_time = frame_start;
                let result = if let Some(t) = &mut self.terrain {
                    t.benchmark_camera(&mut self.flight);
                    t.draw(&self.flight)
                } else if let (Some(g), Some(w)) = (&mut self.gpu, &self.window) {
                    if g.surface.is_none() {
                        g.restore(w.clone())
                            .and_then(|()| g.draw(w, self.yaw, self.pitch))
                    } else {
                        g.draw(w, self.yaw, self.pitch)
                    }
                } else {
                    Ok(false)
                };
                match result {
                    Ok(true) => {
                        if matches!(self.initialization, Initialization::UploadingGeometry)
                            && self.terrain.as_ref().is_some_and(|t| t.sections > 0)
                        {
                            self.transition(Initialization::Ready);
                        }
                        self.sample(frame_start.elapsed().as_secs_f64());
                    }
                    Ok(false) => {}
                    Err(e) => self.fail(e),
                }
                let period = Duration::from_secs_f64(1. / 60.);
                self.next_frame += period;
                // Hold a fixed cadence despite small timer delays; skip missed
                // deadlines instead of issuing bursts after a long stall.
                if self.next_frame <= frame_start {
                    self.next_frame = frame_start + period;
                }
            }
            _ => {}
        }
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(game) = &mut self.gameplay {
            game.exiting(event_loop);
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: HostEvent) {
        match event {
            HostEvent::LauncherAction => {}
            HostEvent::TouchControls(command, value) => self.touch_command(command, value),
            HostEvent::ResourcesInstalled(root) => {
                self.import = None;
                // Datafix and other upstream tables retain their first root for
                // the process. Activate replacements together on the next launch.
                if self.session_started && self.resource_root.as_ref() != Some(&root) {
                    self.resource_status = "launcher.resources_restart";
                } else {
                    self.resource_root = Some(root);
                    self.resource_status = "launcher.ready";
                }
                status(self.resource_status);
            }
            HostEvent::Play { username, game_dir } => {
                let result = (|| -> Result<()> {
                    if self.resource_status != "launcher.ready" || !self.active {
                        return Err("the launcher is not ready".into());
                    }
                    let root = self
                        .resource_root
                        .as_ref()
                        .ok_or("resources are unavailable")?;
                    let window = self.window.as_ref().ok_or("window is unavailable")?;
                    status("gameplay.loading");
                    std::env::set_current_dir(game_dir)?;
                    self.session_started = true;
                    let mut game = horizon_client::game::mobile::Session::new(
                        root.clone(),
                        window.clone(),
                        &username,
                    )?;
                    game.window_event(
                        event_loop,
                        window.id(),
                        WindowEvent::Resized(drawable_size(window)),
                    );
                    self.gameplay = Some(Box::new(game));
                    Ok(())
                })();
                if let Err(error) = result {
                    eprintln!("Horizon iOS session failed to start: {error}");
                    self.release_instance();
                    status(if self.resource_status == "launcher.ready" {
                        "launcher.error"
                    } else {
                        self.resource_status
                    });
                }
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let command = LAUNCHER_COMMAND.swap(COMMAND_NONE, Ordering::AcqRel);
        if command == COMMAND_RETURN {
            self.release_gameplay_controls();
            if let Some(game) = &mut self.gameplay {
                match game.begin_return_to_launcher() {
                    Ok(true) => self.finish_session(),
                    Ok(false) => {
                        self.gameplay_stopping = true;
                        native_input::reset();
                        status("gameplay.stopping");
                    }
                    Err(error) => {
                        eprintln!("Horizon iOS session shutdown failed: {error}");
                        status("gameplay.menu");
                    }
                }
            }
            self.gameplay_ready = false;
        }
        if self.gameplay_stopping {
            let finished =
                self.gameplay
                    .as_mut()
                    .is_some_and(|game| match game.finish_return_to_launcher() {
                        Ok(finished) => finished,
                        Err(error) => {
                            eprintln!("Horizon iOS session shutdown failed: {error}");
                            false
                        }
                    });
            if finished {
                self.finish_session();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(50),
                ));
                return;
            }
        }
        if self.gameplay.is_some() {
            #[cfg(target_os = "ios")]
            let pending_text_tap = self.text_tap.as_ref().is_some_and(|(_, id)| {
                native_input::focused_field(self.gameplay.as_mut().unwrap()).as_ref() == Some(id)
            });
            #[cfg(target_os = "ios")]
            if !pending_text_tap {
                self.text_tap = None;
            }
            #[cfg(not(target_os = "ios"))]
            let pending_text_tap = false;
            // Wait for release before changing the UIKit input view during
            // a field-to-field tap. A screen change invalidates the pending tap.
            if !pending_text_tap {
                native_input::sync(
                    self.gameplay.as_mut().unwrap(),
                    self.active,
                    event_loop,
                    None,
                );
            }
            let was_playing = self.gameplay_ready;
            // Native Back/chat can change the screen before this frame's tick.
            self.sync_gameplay_controls();
            if self.gameplay_ready {
                let changes = self.controls.advance(Instant::now(), &self.touch_settings);
                self.apply_gameplay_controls(changes);
            }
            let game = self.gameplay.as_mut().unwrap();
            if let Err(error) = game.update(event_loop) {
                eprintln!("Horizon iOS gameplay failed: {error}");
                self.session_failed = true;
                LAUNCHER_COMMAND.store(COMMAND_RETURN, Ordering::Release);
                event_loop.set_control_flow(ControlFlow::Poll);
                return;
            }
            if !self.gameplay_title_visible && game.title_visible() {
                self.gameplay_title_visible = true;
                status("gameplay.menu");
            }
            // Network events and delayed player creation are handled in update.
            self.sync_gameplay_controls();
            self.sync_touch_ui();
            if self.gameplay_ready && !was_playing {
                let window = self.window.as_ref().unwrap();
                self.gameplay.as_mut().unwrap().window_event(
                    event_loop,
                    window.id(),
                    WindowEvent::Resized(drawable_size(window)),
                );
                if !matches!(self.initialization, Initialization::Ready) {
                    self.transition(Initialization::Ready);
                }
            }
            let saving = self
                .gameplay
                .as_ref()
                .is_some_and(|game| game.saving_in_background());
            if !saving {
                background_save(false);
            }
            if !self.active {
                event_loop.set_control_flow(if saving {
                    ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50))
                } else {
                    ControlFlow::Wait
                });
            }
            return;
        }
        if self.import.as_ref().is_some_and(|job| job.is_finished()) {
            let job = self.import.take().expect("finished import");
            match job.join() {
                Ok(Ok(root)) => {
                    self.resource_root = Some(root);
                    self.resource_status = "launcher.ready";
                    horizon_client::spans::startup::mark("mobile.resources_ready");
                    if !TESTFLIGHT {
                        eprintln!(
                            "MOBILE resources_ready_ms={:.2}",
                            horizon_ios_startup_elapsed_ms()
                        );
                    }
                }
                Ok(Err(e)) => {
                    self.resource_status = match e.kind() {
                        std::io::ErrorKind::NotFound => "launcher.resources_missing",
                        std::io::ErrorKind::Unsupported => "launcher.resources_incompatible",
                        _ => "launcher.resources_invalid",
                    };
                    eprintln!("Prepared resources unavailable: {e}");
                }
                Err(_) => {
                    self.resource_status = "launcher.resources_invalid";
                    eprintln!("Prepared resource import worker panicked");
                }
            }
            if self.active {
                self.resumed(event_loop);
            }
        }
        if !TESTFLIGHT
            && !self.terrain_attempted
            && self.resource_root.is_some()
            && std::env::var("HORIZON_IOS_SCENE").as_deref() == Ok("terrain")
        {
            self.terrain_attempted = true;
            self.transition(Initialization::PreparingWorld);
            let root = self.resource_root.clone().unwrap();
            match std::thread::Builder::new()
                .name("Terrain preparation".into())
                .spawn(move || terrain::Prepared::load(&root).map_err(|e| e.to_string()))
            {
                Ok(job) => self.preparation = Some(job),
                Err(e) => {
                    self.transition(Initialization::Failed);
                    eprintln!("TERRAIN worker failed: {e}");
                }
            }
        }
        if self
            .preparation
            .as_ref()
            .is_some_and(|job| job.is_finished())
        {
            match self.preparation.take().unwrap().join() {
                Ok(Ok(prepared)) => self.prepared = Some(prepared),
                Ok(Err(e)) => {
                    self.transition(Initialization::Failed);
                    eprintln!("TERRAIN preparation failed: {e}");
                }
                Err(_) => {
                    self.transition(Initialization::Failed);
                    eprintln!("TERRAIN preparation worker panicked");
                }
            }
            if self.active {
                self.resumed(event_loop);
            }
        }
        let diagnostics = !TESTFLIGHT
            && matches!(
                std::env::var("HORIZON_IOS_SCENE").as_deref(),
                Ok("terrain" | "synthetic")
            );
        if self.active && (diagnostics || self.gameplay.is_some()) {
            if Instant::now() >= self.next_frame
                && let Some(w) = &self.window
            {
                w.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        } else if self.import.is_some() || self.preparation.is_some() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(50),
            ));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn background_save(begin: bool) {
    #[cfg(target_os = "ios")]
    {
        unsafe extern "C" {
            fn horizon_ios_background_save_begin();
            fn horizon_ios_background_save_end();
        }
        // SAFETY: only called by the main UIKit event-loop thread.
        unsafe {
            if begin {
                horizon_ios_background_save_begin();
            } else {
                horizon_ios_background_save_end();
            }
        }
    }
    #[cfg(not(target_os = "ios"))]
    let _ = begin;
}

fn status(key: &str) {
    #[cfg(target_os = "ios")]
    {
        unsafe extern "C" {
            fn horizon_ios_status(key: *const std::ffi::c_char);
        }
        let key = std::ffi::CString::new(key).unwrap();
        // SAFETY: called on the main event-loop thread; the C function copies the string.
        unsafe {
            horizon_ios_status(key.as_ptr());
        }
    }
    #[cfg(not(target_os = "ios"))]
    eprintln!("{key}");
}

/// Called exactly once by ios/App/main.m on the main thread. Paths are copied
/// before entering winit, which calls UIApplicationMain and owns UIKit lifecycle.
///
/// # Safety
/// Each argument must be a live, NUL-terminated UTF-8 absolute path for this call.
/// `documents` is the app's Documents directory, where a player may place a
/// `HorizonResources` folder instead of embedding prepared data in the bundle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_run(
    support: *const std::ffi::c_char,
    cache: *const std::ffi::c_char,
    bundle: *const std::ffi::c_char,
    documents: *const std::ffi::c_char,
) {
    let launched = *APP_STARTED.get_or_init(Instant::now);
    if log::set_logger(&NATIVE_LOG).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    std::panic::set_hook(Box::new(|info| {
        eprintln!(
            "Horizon Rust panic: {info}\n{}",
            std::backtrace::Backtrace::force_capture()
        )
    }));
    // Never unwind across the C boundary.
    let result = std::panic::catch_unwind(|| -> Result<()> {
        // SAFETY: the caller guarantees valid strings; owned copies outlive this scope.
        let path = |p| {
            unsafe { std::ffi::CStr::from_ptr(p) }
                .to_str()
                .map(std::path::PathBuf::from)
        };
        let (support, cache, bundle, documents) = (
            path(support)?,
            path(cache)?,
            path(bundle)?,
            path(documents)?,
        );
        std::fs::create_dir_all(&cache)?;
        let (parent, legacy, home) =
            if !TESTFLIGHT && let Ok(session) = std::env::var("HORIZON_IOS_TEST_SESSION") {
                if session.is_empty()
                    || session.len() > 64
                    || !session
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                {
                    return Err("invalid disposable iOS session name".into());
                }
                let parent = support.join("Tests").join(session);
                let home = parent.join(format!(
                    "run-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_nanos()
                ));
                (parent.clone(), parent, home)
            } else {
                (
                    support.clone(),
                    support.join("Gameplay"),
                    support.join("logs"),
                )
            };
        std::fs::create_dir_all(&parent)?;
        std::env::set_current_dir(&parent)?;
        INSTANCE_STORE
            .set(instances::Store { parent, legacy })
            .map_err(|_| "instance store already initialized")?;
        let storage = support.join("resources");
        RESOURCE_STORE
            .set(storage.clone())
            .map_err(|_| "resource store already initialized")?;
        // The upstream behaviour cache is process-wide. Keep it outside any
        // individual instance so deleting an instance cannot invalidate it.
        horizon_world::level::Behaviours::set_cache_dir(cache.join("behaviours"));
        std::fs::create_dir_all(&home)?;
        for (name, leaf) in [
            ("HORIZON_HOME", "home"),
            ("HORIZON_LAUNCHER_HOME", "launcher"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CONFIG_HOME", "config"),
            ("HORIZON_LAUNCHER_TRASH", "trash"),
            ("HORIZON_LAUNCHER_CLIPBOARD", "clipboard"),
            ("HORIZON_LAUNCHER_DESKTOP", "desktop"),
        ] {
            let path = home.join(leaf);
            std::fs::create_dir_all(&path)?;
            // Before the importer, event loop or game workers exist.
            unsafe {
                std::env::set_var(name, path);
            }
        }
        unsafe {
            std::env::set_var("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1");
            // Audio uses the shared Rust mixer and iOS Core Audio output.
        }
        if TESTFLIGHT {
            // Before any workers can read the inherited environment.
            unsafe { std::env::remove_var("HORIZON_STARTUP_TRACE") };
        } else if std::env::var("HORIZON_IOS_STARTUP_TRACE").as_deref() == Ok("1") {
            // Still before any workers; always keep the optional trace in the sandbox.
            unsafe {
                std::env::set_var("HORIZON_STARTUP_TRACE", home.join("startup-trace.jsonl"));
            }
        }
        horizon_client::logging::set_game_home(home);
        horizon_client::spans::startup::init(launched);
        // Enter UIApplicationMain immediately. A first import can involve tens
        // of thousands of files and must not hold up the first UIKit frame.
        let importer = std::thread::Builder::new()
            .name("Resource import".into())
            .spawn(move || {
                let started = Instant::now();
                if !TESTFLIGHT {
                    eprintln!(
                        "RESOURCE_IMPORT started; Horizon {}",
                        horizon_core::VANILLA_VERSION
                    );
                }
                let _import = RESOURCE_IO
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let result = resources::available(&bundle, &documents, &storage);
                if !TESTFLIGHT {
                    eprintln!(
                        "RESOURCE_IMPORT elapsed_ms={:.2} result={result:?}",
                        started.elapsed().as_secs_f64() * 1000.
                    );
                }
                result
            })?;
        let mut app = App {
            launched,
            import: Some(importer),
            ..App::default()
        };
        app.transition(Initialization::LoadingResources);
        let event_loop = EventLoop::<HostEvent>::with_user_event().build()?;
        EVENT_PROXY
            .set(event_loop.create_proxy())
            .map_err(|_| "iOS event proxy already initialized")?;
        event_loop.run_app(&mut app)?;
        Ok(())
    });
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            eprintln!("Horizon startup failed: {e}");
            std::process::abort();
        }
        Err(_) => std::process::abort(),
    }
}

static SENSITIVITY: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0.18_f32.to_bits());
/// UIKit sensitivity slider, in degrees per point.
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_sensitivity(value: f32) {
    if value.is_finite() {
        SENSITIVITY.store(
            value.clamp(0.03, 0.6).to_bits(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}
