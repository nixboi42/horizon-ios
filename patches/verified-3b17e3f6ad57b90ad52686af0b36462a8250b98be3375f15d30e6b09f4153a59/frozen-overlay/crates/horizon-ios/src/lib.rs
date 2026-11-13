//! Native gameplay host with explicitly selected terrain/synthetic diagnostics.
//! winit owns UIKit; this host imports resources and forwards lifecycle/input to Game.
mod flight;
mod gameplay_input;
mod native_input;
mod resources;
mod scene;
mod terrain;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
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
    menu_touch: Option<u64>,
    gameplay_ready: bool,
    resource_root: Option<std::path::PathBuf>,
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
            menu_touch: None,
            gameplay_ready: false,
            resource_root: None,
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
    fn apply_gameplay_controls(&mut self, changes: gameplay_input::Changes) {
        if let Some(game) = &mut self.gameplay {
            for (action, down) in changes.actions {
                if let Err(error) = game.action(action, down) {
                    eprintln!("MOBILE input failed: {error}");
                }
            }
            game.look(changes.look);
        }
    }
    fn release_gameplay_controls(&mut self) {
        let changes = self.controls.clear();
        self.apply_gameplay_controls(changes);
        self.menu_touch = None;
    }
    fn sync_gameplay_controls(&mut self) {
        let playing = self.active
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
        let w = self.window.as_ref().unwrap();
        if self.gameplay_ready {
            let scale = w.scale_factor();
            let size = drawable_size(w);
            let position = [t.location.x, t.location.y];
            let slot = if t.phase == TouchPhase::Started {
                let game = self.gameplay.as_mut().unwrap();
                game.set_hud_safe_area(safe_area_bottom_points(w), scale);
                game.hotbar_slot_at(position)
            } else {
                None
            };
            let changes = self.controls.touch(
                t.id,
                t.phase,
                position.map(|px| px / scale),
                [size.width as f64 / scale, size.height as f64 / scale],
                slot,
            );
            self.apply_gameplay_controls(changes);
        } else {
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
                game.window_event(
                    el,
                    id,
                    WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: if t.phase == TouchPhase::Started {
                            ElementState::Pressed
                        } else {
                            ElementState::Released
                        },
                        button: MouseButton::Left,
                    },
                );
            }
            if matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.menu_touch = None;
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
impl ApplicationHandler for App {
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
            if let Err(error) = game.resume() {
                self.fail(error);
            }
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
        eprintln!(
            "Horizon iOS memory warning: {:?}",
            horizon_threads::process_memory::resident_bytes()
        );
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.gameplay.is_some() {
            // UIKit owns typing, but releases must still clear physical keys
            // held before the field opened (otherwise movement can stick).
            let native_text_event = matches!(event, WindowEvent::Ime(_))
                || matches!(&event, WindowEvent::KeyboardInput { event, .. }
                    if event.state == winit::event::ElementState::Pressed);
            if native_text_event && native_input::owns_text() {
                return;
            }
            if let WindowEvent::Touch(touch) = event {
                #[cfg(target_os = "ios")]
                if touch.phase == winit::event::TouchPhase::Started {
                    unsafe extern "C" {
                        fn horizon_ios_keyboard_reopen();
                    }
                    unsafe {
                        horizon_ios_keyboard_reopen();
                    }
                }
                // Apply the old field's committed edits before this touch can
                // select a new field or replace its screen.
                native_input::sync(self.gameplay.as_mut().unwrap(), self.active, event_loop);
                self.gameplay_touch(event_loop, id, touch);
            } else {
                if matches!(event, WindowEvent::Focused(false) | WindowEvent::Resized(_)) {
                    self.release_gameplay_controls();
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
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.gameplay.is_some() {
            native_input::sync(self.gameplay.as_mut().unwrap(), self.active, event_loop);
            let was_playing = self.gameplay_ready;
            // Native Back/chat can change the screen before this frame's tick.
            self.sync_gameplay_controls();
            let result = self.gameplay.as_mut().unwrap().update(event_loop);
            // Network events and delayed player creation are handled in update.
            self.sync_gameplay_controls();
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
            if let Err(error) = result {
                self.fail(error);
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
                Ok(Ok(root)) => self.resource_root = Some(root),
                Ok(Err(e)) => {
                    self.transition(Initialization::Failed);
                    eprintln!("Prepared resources unavailable: {e}");
                }
                Err(_) => {
                    self.transition(Initialization::Failed);
                    eprintln!("Prepared resource import worker panicked");
                }
            }
            if self.active {
                self.resumed(event_loop);
            }
        }
        if self.active
            && !self.terrain_attempted
            && self.resource_root.is_some()
            && !matches!(
                std::env::var("HORIZON_IOS_SCENE").as_deref(),
                Ok("terrain" | "synthetic")
            )
        {
            self.terrain_attempted = true;
            self.gpu = None;
            self.flight.clear();
            status("gameplay.loading");
            let root = self.resource_root.clone().unwrap();
            let window = self.window.as_ref().unwrap().clone();
            match horizon_client::game::mobile::Session::new(root, window) {
                Ok(mut game) => {
                    let window = self.window.as_ref().unwrap();
                    game.window_event(
                        event_loop,
                        window.id(),
                        WindowEvent::Resized(drawable_size(window)),
                    );
                    self.gameplay = Some(Box::new(game));
                }
                Err(error) => self.fail(error),
            }
            return;
        }
        if !self.terrain_attempted
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
        if self.active {
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
        // Upstream resolves saves/options/packs from cwd. Set it once, before any
        // threads exist, to a dedicated app-owned folder, never the resource tree.
        let game_dir = if let Ok(session) = std::env::var("HORIZON_IOS_TEST_SESSION") {
            if session.is_empty()
                || session.len() > 64
                || !session
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err("invalid disposable iOS session name".into());
            }
            support.join("Tests").join(session)
        } else {
            support.join("Gameplay")
        };
        std::fs::create_dir_all(game_dir.join("saves"))?;
        std::env::set_current_dir(&game_dir)?;
        let home = if std::env::var_os("HORIZON_IOS_TEST_SESSION").is_some() {
            game_dir.join(format!(
                "run-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ))
        } else {
            support.join("logs")
        };
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
            std::env::set_var("HORIZON_NO_AUDIO", "1");
        }
        horizon_client::logging::set_game_home(home);
        horizon_client::spans::startup::init(Instant::now());
        // Enter UIApplicationMain immediately. A first import can involve tens
        // of thousands of files and must not hold up the first UIKit frame.
        let importer = std::thread::Builder::new()
            .name("Resource import".into())
            .spawn(move || {
                let started = Instant::now();
                eprintln!(
                    "RESOURCE_IMPORT started; Horizon {}",
                    horizon_core::VANILLA_VERSION
                );
                let result =
                    resources::import_from(&bundle, &documents, &support.join("resources"));
                eprintln!(
                    "RESOURCE_IMPORT elapsed_ms={:.2} result={result:?}",
                    started.elapsed().as_secs_f64() * 1000.
                );
                result
            })?;
        let mut app = App {
            import: Some(importer),
            ..App::default()
        };
        app.transition(Initialization::LoadingResources);
        EventLoop::new()?.run_app(&mut app)?;
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
