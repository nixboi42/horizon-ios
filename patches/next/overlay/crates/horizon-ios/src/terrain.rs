//! Native viewer using Horizon's real generation, lighting, streaming and renderer.
use client::{
    assets::{Assets, RenderAssets},
    biome::Biomes,
    blocks::BlockFacts,
    gfx::{Frame, Renderer},
    graphics::GraphicsApi,
    hud::HudBatch,
    light::LightTables,
    mesher::{MeshTables, SectionMesh},
    models,
    streaming::{Streaming, WorkerContext},
    vanillagen::VanillaTerrain,
    world::World,
};
use horizon_client_lib as client;
use horizon_world::registry::BlockStateId;
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::Arc,
    time::Instant,
};
use winit::window::Window;

/// Built once on a named worker. No UIKit, surface or GPU objects cross threads.
pub struct Prepared {
    render: Arc<RenderAssets>,
    streaming: Streaming,
    world: World,
    sky: client::environment::Sky,
    start: glam::DVec3,
}
impl Prepared {
    pub fn load(root: &Path) -> crate::Result<Self> {
        let started = Instant::now();
        horizon_threads::jobs::configure(2);
        client::streaming::job_stats::enable(true);
        let assets = Arc::new(Assets::load(root)?);
        eprintln!(
            "STARTUP registry_ms={:.2}",
            started.elapsed().as_secs_f64() * 1000.
        );
        let decoding = Instant::now();
        let render = Arc::new(assets.reload(root)?);
        eprintln!(
            "STARTUP textures_models_ms={:.2}",
            decoding.elapsed().as_secs_f64() * 1000.
        );
        let facts = BlockFacts::load(root, assets.registry)?;
        let biomes = Arc::new(Biomes::load(root)?);
        let visibility = models::load_visibility(root, assets.registry)?;
        let context = Arc::new(WorkerContext {
            tables: Arc::new(MeshTables::new(&assets, &render, &facts, visibility)?),
            light: Arc::new(LightTables::load(root, assets.registry, &facts)?),
            biomes: biomes.clone(),
            assets: assets.clone(),
            render: render.clone(),
        });
        let generating = Instant::now();
        let generator =
            VanillaTerrain::new(&root.join("worldgen"), assets.registry, &biomes, 1234)?;
        let origin = generator.origin()?;
        let mut streaming = Streaming::new(Arc::new(generator), context, 2);
        streaming.center = (origin.x, origin.z);
        streaming.radius = std::env::var("HORIZON_IOS_RADIUS")
            .ok()
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(2)
            .clamp(2, 3);
        streaming.biome_zoom =
            horizon_core::biome::BiomeZoom::new(horizon_core::biome::obfuscate_seed(1234));
        streaming.translucent_layers = render.translucent_layers.clone().into();
        streaming.mesh = false;
        let mut world = World::new(BlockStateId(0), 1234);
        // Wait only on the preparation worker. Streaming's workers own actual
        // generation, decoration, neighbor lighting and section meshing.
        while !world.chunks.get(&streaming.center).is_some_and(|c| c.lit) {
            let (meshes, _) = streaming.update(&mut world);
            streaming.free(meshes);
            if generating.elapsed().as_secs() > 120 {
                return Err("initial terrain generation timed out".into());
            }
            streaming.wait(std::time::Duration::from_millis(5));
        }
        streaming.mesh = true;
        // The headless preparation phase clears dirty mesh requests. Restore
        // them for every already-lit column when enabling renderer work.
        for (&(x, z), chunk) in world.chunks.iter() {
            for sy in 0..chunk.sections.len() {
                world.dirty_sections.insert((x, sy as i32, z));
            }
        }
        let chunk = &world.chunks[&streaming.center];
        let start = glam::DVec3::new(
            f64::from(origin.x * 16 + 8),
            f64::from(chunk.world_surface(8, 8, |id| id == 0) + 6),
            f64::from(origin.z * 16 + 8),
        );
        eprintln!(
            "STARTUP initial_world_ms={:.2} cpu_prepare_ms={:.2}",
            generating.elapsed().as_secs_f64() * 1000.,
            started.elapsed().as_secs_f64() * 1000.
        );
        Ok(Self {
            render,
            streaming,
            world,
            start,
            sky: client::environment::Environment::load(root)?.sample(&client::environment::Sample {
                day_time: 6000,
                ..Default::default()
            }),
        })
    }
}

pub struct Terrain {
    pub renderer: Renderer,
    pub start: glam::DVec3,
    pub sections: usize,
    pub triangles: usize,
    pub chunks: usize,
    prepared: Prepared,
    pending: VecDeque<((i32, i32, i32), SectionMesh)>,
    geometry: HashMap<(i32, i32, i32), usize>,
    first_frame: bool,
    benchmark: Option<(Instant, glam::DVec3, u64)>,
    benchmark_enabled: bool,
}
impl Terrain {
    pub async fn new(window: Arc<Window>, prepared: Prepared) -> crate::Result<Self> {
        let started = Instant::now();
        let render = &prepared.render;
        let size = crate::drawable_size(&window);
        let mut renderer = Renderer::new(
            window,
            false,
            &render.pixels.get(),
            render.sprite_size,
            render.sprite_minimum_size,
            render.texture_layers,
            &render.sprite_mips,
            &render.animations,
            (&[255; 4], 1, 1),
            (&[0; 4], 1, 1),
            (&[0; 4], 1, 1),
            (&[0; 4], 1),
            Arc::new(client::gfx::panorama::Faces::colour([0; 4])),
            None,
            None,
            GraphicsApi::Auto,
            false,
            false,
            &client::graphics::Graphics::default(),
            Default::default(),
            Default::default(),
        )
        .await?;
        render.pixels.pack();
        renderer.set_pass_timing(true);
        renderer.resize(size.width.max(1), size.height.max(1));
        eprintln!(
            "STARTUP main_gpu_init_ms={:.2}",
            started.elapsed().as_secs_f64() * 1000.
        );
        Ok(Self {
            renderer,
            start: prepared.start,
            sections: 0,
            triangles: 0,
            chunks: 0,
            prepared,
            pending: VecDeque::new(),
            geometry: HashMap::new(),
            first_frame: false,
            benchmark: None,
            benchmark_enabled: std::env::var("HORIZON_IOS_BENCH").as_deref() == Ok("1"),
        })
    }
    fn stream(&mut self, camera: glam::DVec3) {
        let started = Instant::now();
        let center = camera_chunk(camera);
        if self.prepared.streaming.center != center {
            // Revoke pending main-thread results on a new camera generation.
            // Re-dirty retained sections so a rapid A -> B -> A cannot lose them.
            let stale: Vec<_> = self.pending.drain(..).collect();
            for (key, _) in &stale {
                self.prepared.world.dirty_sections.insert(*key);
            }
            self.prepared.streaming.free(stale);
            self.prepared.streaming.center = center;
        }
        // Do not drain another mesh batch until the previous bounded worker
        // batch is uploaded. Never accumulate frame-over-frame upload queues.
        if self.pending.is_empty() {
            let (meshes, removed) = self.prepared.streaming.update(&mut self.prepared.world);
            let removed: Vec<_> = removed.iter().map(|c| (c.x, c.z)).collect();
            self.renderer.remove_columns(removed.iter().copied());
            self.geometry
                .retain(|&(x, _, z), _| !removed.contains(&(x, z)));
            self.pending.extend(meshes);
        }
        let upload = Instant::now();
        let mut count = 0;
        while count < 2 && upload.elapsed().as_secs_f64() < 0.002 {
            let Some((key, mesh)) = self.pending.pop_front() else {
                break;
            };
            if self.prepared.world.chunks.contains_key(&(key.0, key.2)) {
                self.renderer.upload(key, &mesh);
                self.geometry
                    .insert(key, (mesh.solid.len() + mesh.translucent.len()) / 4 * 2);
            }
            self.prepared.streaming.free(vec![(key, mesh)]);
            count += 1;
        }
        self.sections = self.geometry.values().filter(|&&n| n > 0).count();
        self.triangles = self.geometry.values().sum();
        self.chunks = self.prepared.world.chunks.len();
        if count > 0 || started.elapsed().as_millis() > 8 {
            eprintln!(
                "STREAM center={center:?} loaded={} pending={} uploaded={count} coordinator_upload_ms={:.2} upload_ms={:.2} generated={} meshed={}",
                self.chunks,
                self.pending.len(),
                started.elapsed().as_secs_f64() * 1000.,
                upload.elapsed().as_secs_f64() * 1000.,
                self.prepared.streaming.generated_total,
                self.prepared.streaming.meshed_total
            );
        }
    }
    /// Opt-in reproducible camera route; never sends simulation/player actions.
    pub fn benchmark_camera(&mut self, camera: &mut crate::flight::Flight) {
        if !self.benchmark_enabled || !self.first_frame {
            return;
        }
        let (started, origin, previous) =
            self.benchmark
                .get_or_insert((Instant::now(), self.start, u64::MAX));
        let seconds = started.elapsed().as_secs_f64();
        let (phase, offset) = if seconds < 30. {
            (0, 0.)
        } else if seconds < 90. {
            (1, (seconds - 30.) * 8.)
        } else if seconds < 120. {
            (2, 480. + ((seconds - 90.) / 5.).floor() * 64.)
        } else {
            (3, 0.)
        };
        camera.position = *origin + glam::DVec3::new(offset, 0., 0.);
        if *previous != phase {
            eprintln!(
                "BENCH phase={phase} elapsed_s={seconds:.2} (0=static 1=movement 2=rapid 3=return)"
            );
            *previous = phase;
        }
    }
    pub fn draw(&mut self, camera: &crate::flight::Flight) -> crate::Result<bool> {
        let started = Instant::now();
        self.stream(camera.position);
        let hud = HudBatch::new([1, 1]);
        let mut frame = terrain_frame(&hud);
        frame.sky = self.prepared.sky;
        frame.camera = camera.position;
        frame.eye = camera.position;
        frame.yaw = camera.yaw;
        frame.pitch = camera.pitch;
        frame.eye_rotation = [camera.yaw, camera.pitch];
        let result = self.renderer.render(&frame);
        if result.as_ref().is_ok_and(|&visible| visible) && self.sections > 0 && !self.first_frame {
            eprintln!(
                "STARTUP first_visible_terrain sections={} triangles={} first_draw_ms={:.2}",
                self.sections,
                self.triangles,
                started.elapsed().as_secs_f64() * 1000.
            );
            self.first_frame = true;
        }
        result
    }
}
fn camera_chunk(position: glam::DVec3) -> (i32, i32) {
    // Arithmetic shift after flooring is essential at negative boundaries.
    (
        (position.x.floor() as i32) >> 4,
        (position.z.floor() as i32) >> 4,
    )
}
fn terrain_frame(hud: &HudBatch) -> Frame<'_> {
    Frame {
        weather: &[],
        weather_time: client::weather::Time::new(0, 0.0),
        camera: glam::DVec3::new(0.5, 100.0, 0.5),
        yaw: 0.0,
        pitch: 10.0,
        fov_degrees: 85.0,
        eye: glam::DVec3::new(0.5, 100.0, 0.5),
        eye_rotation: [0.0, 10.0],
        first_person: true,
        bob: glam::Mat4::IDENTITY,
        spin: glam::Mat4::IDENTITY,
        sky: client::environment::Sky::default(),
        shadow_scene_revision: (0, 0),
        shadow_cache: false,
        distance: 128.0,
        detail_distance: 128.0,
        depth_far: 1000.0,
        underwater: false,
        water_plane: None,
        cave_culling: false,
        title: false,
        sky_blocked: false,
        skybox: false,
        end_sky: false,
        end_sky_sprite: None,
        boss_world_fog: false,
        environment_fog: None,
        water_fog: [0.0; 3],
        fog: None,
        dark_disc: false,
        outline: Vec::new(),
        outline_after_terrain: false,
        outline_high_contrast: false,
        pvp: Default::default(),
        debug_lines: &[],
        debug_lines_on_top: &[],
        debug_crosshair: &[],
        hud,
        glint_alpha: 0.75,
        glint_ticks: 0.0,
        nether_lighting: false,
        entities: &[],
        static_block_entities: &[],
        visible_static_block_entity_sections: &[],
        static_block_entity_stats: Default::default(),
        entity_motion: &[],
        entity_motion_posed: &[],
        entity_spans: &[],
        skin: None,
        glowing: &[],
        dynamic_lights: &[],
        colored_lights: &[],
        emissive_layers: &[],
        clouds: None,
        lightning: Default::default(),
        additive: &[],
        see_through: &[],
        world_primitives: Default::default(),
        shadow_only: &[],
        celestial: &[],
        stars: None,
        hand: &[],
        hand_items: &[],
        hand_fov_degrees: 70.0,
        screen: &[],
        pack: Default::default(),
        border: None,
        game_time: 0.0,
        shader_game_time: 0.0,
        end_portal_sprites: [[0.0; 4]; 2],
        crumbling_sprite: [0.0; 4],
        ticks: 0,
        darkness: (0.0, 0.0),
        night_vision: 0.0,
        block_flicker: 0.0,
        boss_darken: 0.0,
        chunk_fade: 0.0,
        post_effects: client::post_chain::Request::none(),
        menu_blur: None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn camera_crosses_positive_and_negative_chunk_boundaries() {
        for (x, expected) in [
            (-16.01, -2),
            (-16., -1),
            (-0.01, -1),
            (0., 0),
            (15.99, 0),
            (16., 1),
        ] {
            assert_eq!(
                super::camera_chunk(glam::DVec3::new(x, 100., x)),
                (expected, expected)
            );
        }
    }
    #[test]
    #[ignore = "requires private prepared 26.3 resources; sets HORIZON_IOS_TEST_RESOURCES"]
    fn real_world_streams_meshes_and_evicts_after_camera_travel() {
        use std::{
            path::PathBuf,
            time::{Duration, Instant},
        };
        let root = PathBuf::from(
            std::env::var_os("HORIZON_IOS_TEST_RESOURCES").expect("explicit resources"),
        );
        let mut prepared = super::Prepared::load(&root).unwrap();
        let origin = prepared.streaming.center;
        let mut initial_meshes = std::collections::HashSet::new();
        for center in [
            origin,
            (origin.0 + 12, origin.1),
            (origin.0 - 12, origin.1),
            origin,
        ] {
            prepared.streaming.center = center;
            let started = Instant::now();
            let mut seen = std::collections::HashSet::new();
            let mut evicted = 0;
            loop {
                let (meshes, removed) = prepared.streaming.update(&mut prepared.world);
                evicted += removed.len();
                for (key, mesh) in &meshes {
                    if !mesh.solid.is_empty() || !mesh.translucent.is_empty() {
                        seen.insert((key.0, key.2));
                    }
                }
                prepared.streaming.free(meshes);
                if seen.contains(&center)
                    && seen.len() >= 5
                    && !prepared
                        .world
                        .chunks
                        .contains_key(&(center.0 + 12, center.1))
                {
                    break;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(90),
                    "streaming stalled at {center:?}: {:?}",
                    prepared.streaming.stages()
                );
                prepared.streaming.wait(Duration::from_millis(5));
            }
            assert!(prepared.world.chunks.len() < 100, "unbounded final columns");
            if center == origin && initial_meshes.is_empty() {
                initial_meshes = seen;
            } else {
                assert!(evicted > 0, "travel must evict old terrain");
            }
            eprintln!(
                "STREAM_TEST center={center:?} loaded={} evicted={evicted} elapsed_ms={:.2}",
                prepared.world.chunks.len(),
                started.elapsed().as_secs_f64() * 1000.
            );
        }
        assert!(initial_meshes.len() >= 5);
    }
}
