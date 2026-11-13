use horizon_assets::{
    model::{CuboidModel, Direction},
    resolve::ModelLibrary,
};
use std::collections::BTreeMap;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

/// Original synthetic model JSON, decoded, resolved and baked by the same
/// horizon-assets code used by the game. No Minecraft assets or world data.
pub fn diagnostic_mesh() -> super::Result<Vec<Vertex>> {
    let faces: BTreeMap<_, _> = Direction::ALL
        .into_iter()
        .map(|d| (d.name(), serde_json::json!({"texture":"#all"})))
        .collect();
    let json = serde_json::json!({"textures":{"all":"horizon:diagnostic"}, "elements":[{"from":[0,0,0],"to":[16,16,16],"faces":faces}]}).to_string();
    let model = CuboidModel::from_json(&json)?;
    let library = ModelLibrary::new(BTreeMap::from([("horizon:diagnostic".into(), model)]))?;
    let quads = horizon_assets::bake::bake_model(&library.resolve("horizon:diagnostic")?)?;
    let mut vertices = Vec::with_capacity(quads.len() * 6);
    for q in quads {
        let color = match q.facing {
            Direction::Up => [0.4, 0.85, 0.5],
            Direction::Down => [0.2, 0.3, 0.4],
            Direction::North | Direction::South => [0.3, 0.55, 0.85],
            _ => [0.75, 0.45, 0.3],
        };
        for i in [0, 1, 2, 0, 2, 3] {
            vertices.push(Vertex {
                position: q.positions[i].map(|p| p - 0.5),
                color,
            });
        }
    }
    Ok(vertices)
}
