//! Development-only extraction; opens the source region read-only and never overwrites output.
use horizon_core::position::ChunkPos;
use horizon_world::{
    nbt::{self, Limits, Tag},
    region::RegionFile,
};
use std::{fs::OpenOptions, io::Write, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 3 && (args[0] == "--generate" || args[0] == "--generate-spawn") {
        return generate(
            &PathBuf::from(&args[1]),
            &PathBuf::from(&args[2]),
            args[0] == "--generate-spawn",
        );
    }
    if args.len() != 4 {
        return Err("usage: horizon-ios-fixture REGION.mca CHUNK_X CHUNK_Z OUTPUT.nbt (disposable 26.3 Overworld only); or --generate RESOURCES OUTPUT.nbt".into());
    }
    let x = args[1].to_str().ok_or("invalid x")?.parse()?;
    let z = args[2].to_str().ok_or("invalid z")?.parse()?;
    let mut region = RegionFile::open(PathBuf::from(&args[0]))?;
    let root = region
        .read(ChunkPos { x, z })?
        .ok_or("chunk absent")?
        .nbt(Limits::default())?;
    if root.tag.get("DataVersion") != Some(&Tag::Int(horizon_core::version::DATA_VERSION)) {
        return Err("chunk is not the pinned 26.3 data version".into());
    }
    if root.tag.get("isLightOn") != Some(&Tag::Byte(1)) {
        return Err("chunk has no saved lighting; finish generation and save it first".into());
    }
    let bytes = nbt::write_named(&root, Limits::default())?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(PathBuf::from(&args[3]))?
        .write_all(&bytes)?;
    eprintln!(
        "Exported {} bytes without modifying the source region",
        bytes.len()
    );
    Ok(())
}

/// Generate a disposable chunk using the game's actual generation/decoration/light coordinator.
fn generate(
    resources: &std::path::Path,
    output: &std::path::Path,
    at_spawn: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use horizon_client_lib::{
        assets::Assets,
        biome::Biomes,
        blocks::BlockFacts,
        light::LightTables,
        mesher::MeshTables,
        models, save,
        streaming::{Streaming, WorkerContext},
        vanillagen::VanillaTerrain,
        world::World,
    };
    use horizon_world::registry::BlockStateId;
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    if output.exists() {
        return Err("refusing to overwrite fixture output".into());
    }
    horizon_threads::jobs::configure(4);
    let assets = Arc::new(Assets::load(resources)?);
    let render = Arc::new(assets.reload(resources)?);
    let facts = BlockFacts::load(resources, assets.registry)?;
    let biomes = Arc::new(Biomes::load(resources)?);
    let visibility = models::load_visibility(resources, assets.registry)?;
    let context = Arc::new(WorkerContext {
        tables: Arc::new(MeshTables::new(&assets, &render, &facts, visibility)?),
        light: Arc::new(LightTables::load(resources, assets.registry, &facts)?),
        biomes: biomes.clone(),
        assets: assets.clone(),
        render,
    });
    let generator =
        VanillaTerrain::new(&resources.join("worldgen"), assets.registry, &biomes, 1234)?;
    let center = if at_spawn {
        let pos = generator.origin()?;
        (pos.x, pos.z)
    } else {
        (0, 0)
    };
    eprintln!("Generating at {center:?}, seed=1234");
    let mut streaming = Streaming::new(Arc::new(generator), context, 4);
    streaming.mesh = false;
    streaming.center = center;
    streaming.radius = 1;
    let mut world = World::new(BlockStateId(0), 1234);
    let started = Instant::now();
    let mut last_report = Instant::now();
    loop {
        let _ = streaming.update(&mut world);
        if world.chunks.get(&center).is_some_and(|chunk| chunk.lit) {
            break;
        }
        if started.elapsed() > Duration::from_secs(600) {
            return Err("generation timed out".into());
        }
        if last_report.elapsed() > Duration::from_secs(5) {
            eprintln!("Generating genuine terrain: {:?}", streaming.stages());
            last_report = Instant::now();
        }
        streaming.wait(Duration::from_millis(2));
    }
    let chunk = &world.chunks[&center];
    let root = save::encode(chunk, assets.registry, &biomes);
    let bytes = nbt::write_named(&root, Limits::default())?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?
        .write_all(&bytes)?;
    eprintln!(
        "Generated seed=1234 chunk={center:?} with Horizon Streaming/VanillaTerrain and saved light in {:.2}s; {} bytes",
        started.elapsed().as_secs_f64(),
        bytes.len()
    );
    Ok(())
}
