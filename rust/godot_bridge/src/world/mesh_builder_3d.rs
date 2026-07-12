use crate::world::render_snapshot::{DynamicRenderSnapshot, RoadRenderCell, SurfaceRenderSnapshot};
use game_engine::components::TerrainKind;
use game_engine::grid::{CellCoord, GridSize};
use game_engine::roads::RoadTier;
use godot::classes::{mesh, ArrayMesh, Material};
use godot::obj::EngineEnum;
use godot::prelude::*;
use std::collections::{HashMap, HashSet};

pub(crate) const WORLD_UNITS_PER_TILE: f32 = 2.0;
pub(crate) const CHUNK_SIZE_TILES: i32 = 32;
pub(crate) const TERRAIN_Y: f32 = 0.0;
pub(crate) const COMPLETED_ROAD_Y: f32 = 0.02;
pub(crate) const PLANNED_ROAD_Y: f32 = 0.03;

const TERRAIN_VARIANT_COLUMNS: i32 = 4;
const ROAD_ATLAS_COLUMNS: u8 = 4;
const ROAD_ATLAS_ROWS: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ChunkCoord {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct UvRect {
    min: Vector2,
    max: Vector2,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct MeshSurfaceGeometry {
    pub(crate) vertices: Vec<Vector3>,
    pub(crate) normals: Vec<Vector3>,
    pub(crate) uvs: Vec<Vector2>,
    pub(crate) indices: Vec<i32>,
}

impl MeshSurfaceGeometry {
    pub(crate) fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    fn append_tile_quad(&mut self, local_x: f32, local_z: f32, y: f32, uv: UvRect) {
        let Ok(first_vertex) = i32::try_from(self.vertices.len()) else {
            return;
        };
        let tile = WORLD_UNITS_PER_TILE;
        self.vertices.extend([
            Vector3::new(local_x, y, local_z),
            Vector3::new(local_x + tile, y, local_z),
            Vector3::new(local_x + tile, y, local_z + tile),
            Vector3::new(local_x, y, local_z + tile),
        ]);
        self.normals.extend([Vector3::UP; 4]);
        self.uvs.extend([
            Vector2::new(uv.min.x, uv.min.y),
            Vector2::new(uv.max.x, uv.min.y),
            Vector2::new(uv.max.x, uv.max.y),
            Vector2::new(uv.min.x, uv.max.y),
        ]);

        // Viewed from +Y, X/Z coordinates map to screen X/down respectively,
        // so this is Godot's clockwise front-face order. Vertex normals remain
        // explicitly upward for lighting even though the right-handed cross
        // product of the winding points down.
        self.indices.extend([
            first_vertex,
            first_vertex + 1,
            first_vertex + 2,
            first_vertex,
            first_vertex + 2,
            first_vertex + 3,
        ]);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TerrainSurfaceGeometry {
    pub(crate) kind: TerrainKind,
    pub(crate) geometry: MeshSurfaceGeometry,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TerrainChunkGeometry {
    pub(crate) chunk: ChunkCoord,
    pub(crate) world_origin: Vector3,
    pub(crate) surfaces: Vec<TerrainSurfaceGeometry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RoadRenderState {
    Completed,
    Planned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RoadSurfaceKey {
    pub(crate) tier: RoadTier,
    pub(crate) state: RoadRenderState,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoadSurfaceGeometry {
    pub(crate) key: RoadSurfaceKey,
    pub(crate) geometry: MeshSurfaceGeometry,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoadChunkGeometry {
    pub(crate) chunk: ChunkCoord,
    pub(crate) world_origin: Vector3,
    pub(crate) surfaces: Vec<RoadSurfaceGeometry>,
}

pub(crate) struct BuiltTerrainChunk {
    pub(crate) chunk: ChunkCoord,
    pub(crate) world_origin: Vector3,
    pub(crate) mesh: Gd<ArrayMesh>,
}

pub(crate) struct BuiltRoadChunk {
    pub(crate) chunk: ChunkCoord,
    pub(crate) world_origin: Vector3,
    pub(crate) mesh: Gd<ArrayMesh>,
}

pub(crate) fn chunk_coord_for_cell(coord: CellCoord) -> ChunkCoord {
    ChunkCoord {
        x: coord.x().div_euclid(CHUNK_SIZE_TILES),
        y: coord.y().div_euclid(CHUNK_SIZE_TILES),
    }
}

pub(crate) fn chunk_origin_cell(chunk: ChunkCoord) -> CellCoord {
    CellCoord::new(
        chunk.x.saturating_mul(CHUNK_SIZE_TILES),
        chunk.y.saturating_mul(CHUNK_SIZE_TILES),
    )
}

pub(crate) fn chunk_world_origin(chunk: ChunkCoord) -> Vector3 {
    let cell = chunk_origin_cell(chunk);
    Vector3::new(
        cell.x() as f32 * WORLD_UNITS_PER_TILE,
        0.0,
        cell.y() as f32 * WORLD_UNITS_PER_TILE,
    )
}

pub(crate) fn chunk_coords_for_size(size: GridSize) -> Vec<ChunkCoord> {
    let chunk_size = CHUNK_SIZE_TILES as usize;
    let width = size.width().div_ceil(chunk_size);
    let height = size.height().div_ceil(chunk_size);
    let width = i32::try_from(width).unwrap_or(i32::MAX);
    let height = i32::try_from(height).unwrap_or(i32::MAX);

    (0..height)
        .flat_map(|y| (0..width).map(move |x| ChunkCoord { x, y }))
        .collect()
}

pub(crate) fn terrain_chunk_coords(snapshot: &SurfaceRenderSnapshot) -> Vec<ChunkCoord> {
    chunk_coords_for_size(snapshot.size)
}

pub(crate) fn road_chunk_coords(snapshot: &DynamicRenderSnapshot) -> Vec<ChunkCoord> {
    let mut chunks = snapshot
        .completed_roads
        .iter()
        .chain(&snapshot.planned_roads)
        .map(|road| chunk_coord_for_cell(road.coord))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    chunks.sort_by_key(|chunk| (chunk.y, chunk.x));
    chunks
}

pub(crate) fn terrain_chunk_geometry(
    snapshot: &SurfaceRenderSnapshot,
    chunk: ChunkCoord,
) -> TerrainChunkGeometry {
    let chunk_origin = chunk_origin_cell(chunk);
    let mut surfaces = Vec::new();

    for kind in TerrainKind::ALL {
        let mut geometry = MeshSurfaceGeometry::default();
        for cell in snapshot
            .terrain_cells
            .iter()
            .filter(|cell| cell.kind == kind && chunk_coord_for_cell(cell.coord) == chunk)
        {
            let local_x = (cell.coord.x() - chunk_origin.x()) as f32 * WORLD_UNITS_PER_TILE;
            let local_z = (cell.coord.y() - chunk_origin.y()) as f32 * WORLD_UNITS_PER_TILE;
            geometry.append_tile_quad(
                local_x,
                local_z,
                TERRAIN_Y,
                terrain_variant_uv(cell.variant),
            );
        }
        if !geometry.is_empty() {
            surfaces.push(TerrainSurfaceGeometry { kind, geometry });
        }
    }

    TerrainChunkGeometry {
        chunk,
        world_origin: chunk_world_origin(chunk),
        surfaces,
    }
}

pub(crate) fn road_chunk_geometry(
    snapshot: &DynamicRenderSnapshot,
    chunk: ChunkCoord,
) -> RoadChunkGeometry {
    let chunk_origin = chunk_origin_cell(chunk);
    let mut surfaces = Vec::new();

    for state in [RoadRenderState::Completed, RoadRenderState::Planned] {
        let roads = match state {
            RoadRenderState::Completed => &snapshot.completed_roads,
            RoadRenderState::Planned => &snapshot.planned_roads,
        };
        let y = match state {
            RoadRenderState::Completed => COMPLETED_ROAD_Y,
            RoadRenderState::Planned => PLANNED_ROAD_Y,
        };

        for tier in RoadTier::ALL {
            let mut geometry = MeshSurfaceGeometry::default();
            for road in roads
                .iter()
                .filter(|road| road.tier == tier && chunk_coord_for_cell(road.coord) == chunk)
            {
                append_road_quad(&mut geometry, road, chunk_origin, y);
            }
            if !geometry.is_empty() {
                surfaces.push(RoadSurfaceGeometry {
                    key: RoadSurfaceKey { tier, state },
                    geometry,
                });
            }
        }
    }

    RoadChunkGeometry {
        chunk,
        world_origin: chunk_world_origin(chunk),
        surfaces,
    }
}

fn append_road_quad(
    geometry: &mut MeshSurfaceGeometry,
    road: &RoadRenderCell,
    chunk_origin: CellCoord,
    y: f32,
) {
    let local_x = (road.coord.x() - chunk_origin.x()) as f32 * WORLD_UNITS_PER_TILE;
    let local_z = (road.coord.y() - chunk_origin.y()) as f32 * WORLD_UNITS_PER_TILE;
    geometry.append_tile_quad(local_x, local_z, y, road_atlas_uv(road.connectivity_mask));
}

pub(crate) fn build_terrain_chunk_mesh(
    snapshot: &SurfaceRenderSnapshot,
    chunk: ChunkCoord,
    mut material_for: impl FnMut(TerrainKind) -> Option<Gd<Material>>,
) -> Option<BuiltTerrainChunk> {
    let geometry = terrain_chunk_geometry(snapshot, chunk);
    if geometry.surfaces.is_empty() {
        return None;
    }

    let mut mesh = ArrayMesh::new_gd();
    for (surface_index, surface) in geometry.surfaces.iter().enumerate() {
        let surface_index = i32::try_from(surface_index).ok()?;
        add_array_mesh_surface(&mut mesh, &surface.geometry);
        if let Some(material) = material_for(surface.kind) {
            mesh.surface_set_material(surface_index, &material);
        }
    }

    Some(BuiltTerrainChunk {
        chunk,
        world_origin: geometry.world_origin,
        mesh,
    })
}

pub(crate) fn build_road_chunk_mesh(
    snapshot: &DynamicRenderSnapshot,
    chunk: ChunkCoord,
    mut material_for: impl FnMut(RoadSurfaceKey) -> Option<Gd<Material>>,
) -> Option<BuiltRoadChunk> {
    let geometry = road_chunk_geometry(snapshot, chunk);
    if geometry.surfaces.is_empty() {
        return None;
    }

    let mut mesh = ArrayMesh::new_gd();
    for (surface_index, surface) in geometry.surfaces.iter().enumerate() {
        let surface_index = i32::try_from(surface_index).ok()?;
        add_array_mesh_surface(&mut mesh, &surface.geometry);
        if let Some(material) = material_for(surface.key) {
            mesh.surface_set_material(surface_index, &material);
        }
    }

    Some(BuiltRoadChunk {
        chunk,
        world_origin: geometry.world_origin,
        mesh,
    })
}

fn add_array_mesh_surface(mesh: &mut Gd<ArrayMesh>, geometry: &MeshSurfaceGeometry) {
    debug_assert_eq!(geometry.vertices.len(), geometry.normals.len());
    debug_assert_eq!(geometry.vertices.len(), geometry.uvs.len());

    let mut arrays = VarArray::new();
    arrays.resize(mesh::ArrayType::MAX.ord() as usize, &Variant::nil());
    arrays.set(
        mesh::ArrayType::VERTEX.ord() as usize,
        &PackedVector3Array::from(geometry.vertices.as_slice()).to_variant(),
    );
    arrays.set(
        mesh::ArrayType::NORMAL.ord() as usize,
        &PackedVector3Array::from(geometry.normals.as_slice()).to_variant(),
    );
    arrays.set(
        mesh::ArrayType::TEX_UV.ord() as usize,
        &PackedVector2Array::from(geometry.uvs.as_slice()).to_variant(),
    );
    arrays.set(
        mesh::ArrayType::INDEX.ord() as usize,
        &PackedInt32Array::from(geometry.indices.as_slice()).to_variant(),
    );
    mesh.add_surface_from_arrays(mesh::PrimitiveType::TRIANGLES, &arrays);
}

fn terrain_variant_uv(variant: i32) -> UvRect {
    let column = variant.rem_euclid(TERRAIN_VARIANT_COLUMNS) as f32;
    let width = 1.0 / TERRAIN_VARIANT_COLUMNS as f32;
    UvRect {
        min: Vector2::new(column * width, 0.0),
        max: Vector2::new((column + 1.0) * width, 1.0),
    }
}

pub(crate) fn road_atlas_coord(mask: u8) -> (u8, u8) {
    let mask = mask & 0x0f;
    (mask % ROAD_ATLAS_COLUMNS, mask / ROAD_ATLAS_COLUMNS)
}

fn road_atlas_uv(mask: u8) -> UvRect {
    let (column, row) = road_atlas_coord(mask);
    let width = 1.0 / f32::from(ROAD_ATLAS_COLUMNS);
    let height = 1.0 / f32::from(ROAD_ATLAS_ROWS);
    UvRect {
        min: Vector2::new(f32::from(column) * width, f32::from(row) * height),
        max: Vector2::new(f32::from(column + 1) * width, f32::from(row + 1) * height),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RoadVisualKey {
    state: u8,
    y: i32,
    x: i32,
    tier: RoadTier,
    connectivity_mask: u8,
}

pub(crate) fn affected_road_chunks(
    previous: &DynamicRenderSnapshot,
    current: &DynamicRenderSnapshot,
) -> Vec<ChunkCoord> {
    let previous = road_visuals_by_chunk(previous);
    let current = road_visuals_by_chunk(current);
    let mut chunks = previous
        .keys()
        .chain(current.keys())
        .copied()
        .collect::<HashSet<_>>();
    chunks.retain(|chunk| previous.get(chunk) != current.get(chunk));

    let mut chunks = chunks.into_iter().collect::<Vec<_>>();
    chunks.sort_by_key(|chunk| (chunk.y, chunk.x));
    chunks
}

fn road_visuals_by_chunk(
    snapshot: &DynamicRenderSnapshot,
) -> HashMap<ChunkCoord, Vec<RoadVisualKey>> {
    let mut chunks: HashMap<ChunkCoord, Vec<RoadVisualKey>> = HashMap::new();
    for (state, roads) in [
        (0, snapshot.completed_roads.as_slice()),
        (1, snapshot.planned_roads.as_slice()),
    ] {
        for road in roads {
            chunks
                .entry(chunk_coord_for_cell(road.coord))
                .or_default()
                .push(RoadVisualKey {
                    state,
                    y: road.coord.y(),
                    x: road.coord.x(),
                    tier: road.tier,
                    connectivity_mask: road.connectivity_mask,
                });
        }
    }
    for roads in chunks.values_mut() {
        roads.sort_unstable();
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::prelude::Entity;
    use bevy_ecs::world::World;
    use game_engine::roads::Road;
    use game_engine::roads::RoadTier::Cobblestone;
    use game_engine::simulation::GameSimulation;

    #[test]
    fn chunk_helpers_use_thirty_two_tile_row_major_boundaries() {
        assert_eq!(
            chunk_coord_for_cell(CellCoord::new(31, 31)),
            ChunkCoord { x: 0, y: 0 }
        );
        assert_eq!(
            chunk_coord_for_cell(CellCoord::new(32, 31)),
            ChunkCoord { x: 1, y: 0 }
        );
        assert_eq!(
            chunk_coord_for_cell(CellCoord::new(-1, -1)),
            ChunkCoord { x: -1, y: -1 }
        );
        assert_eq!(
            chunk_coords_for_size(GridSize::new(65, 33)),
            vec![
                ChunkCoord { x: 0, y: 0 },
                ChunkCoord { x: 1, y: 0 },
                ChunkCoord { x: 2, y: 0 },
                ChunkCoord { x: 0, y: 1 },
                ChunkCoord { x: 1, y: 1 },
                ChunkCoord { x: 2, y: 1 },
            ]
        );
        assert_eq!(
            chunk_world_origin(ChunkCoord { x: 1, y: 2 }),
            Vector3::new(64.0, 0.0, 128.0)
        );
    }

    #[test]
    fn terrain_geometry_uses_chunk_local_clockwise_quads_and_variant_uvs() {
        let simulation = GameSimulation::new(3);
        let snapshot = SurfaceRenderSnapshot::new(
            simulation.default_surface_id(),
            GridSize::new(64, 32),
            vec![crate::world::render_snapshot::TerrainRenderCell {
                coord: CellCoord::new(32, 1),
                kind: TerrainKind::Grass,
                variant: 2,
            }],
        );
        let chunk = terrain_chunk_geometry(&snapshot, ChunkCoord { x: 1, y: 0 });

        assert_eq!(chunk.world_origin, Vector3::new(64.0, 0.0, 0.0));
        assert_eq!(chunk.surfaces.len(), 1);
        let geometry = &chunk.surfaces[0].geometry;
        assert_eq!(
            geometry.vertices,
            vec![
                Vector3::new(0.0, TERRAIN_Y, 2.0),
                Vector3::new(2.0, TERRAIN_Y, 2.0),
                Vector3::new(2.0, TERRAIN_Y, 4.0),
                Vector3::new(0.0, TERRAIN_Y, 4.0),
            ]
        );
        assert_eq!(geometry.normals, vec![Vector3::UP; 4]);
        assert_eq!(geometry.indices, vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(
            geometry.uvs,
            vec![
                Vector2::new(0.5, 0.0),
                Vector2::new(0.75, 0.0),
                Vector2::new(0.75, 1.0),
                Vector2::new(0.5, 1.0),
            ]
        );

        for triangle in geometry.indices.chunks_exact(3) {
            let a = geometry.vertices[triangle[0] as usize];
            let b = geometry.vertices[triangle[1] as usize];
            let c = geometry.vertices[triangle[2] as usize];
            assert!((b - a).cross(c - a).dot(Vector3::UP) < 0.0);
        }
    }

    #[test]
    fn road_masks_select_row_major_four_by_four_atlas_uvs() {
        assert_eq!(road_atlas_coord(0), (0, 0));
        assert_eq!(road_atlas_coord(5), (1, 1));
        assert_eq!(road_atlas_coord(15), (3, 3));

        let snapshot = DynamicRenderSnapshot {
            planned_roads: vec![RoadRenderCell {
                entity: Entity::PLACEHOLDER,
                coord: CellCoord::new(0, 0),
                tier: Cobblestone,
                connectivity_mask: 5,
            }],
            ..Default::default()
        };
        let chunk = road_chunk_geometry(&snapshot, ChunkCoord { x: 0, y: 0 });
        assert_eq!(chunk.surfaces.len(), 1);
        assert_eq!(
            chunk.surfaces[0].key,
            RoadSurfaceKey {
                tier: Cobblestone,
                state: RoadRenderState::Planned,
            }
        );
        let geometry = &chunk.surfaces[0].geometry;
        assert!(geometry
            .vertices
            .iter()
            .all(|vertex| vertex.y == PLANNED_ROAD_Y));
        assert_eq!(
            geometry.uvs,
            vec![
                Vector2::new(0.25, 0.25),
                Vector2::new(0.5, 0.25),
                Vector2::new(0.5, 0.5),
                Vector2::new(0.25, 0.5),
            ]
        );
    }

    #[test]
    fn terrain_and_road_surfaces_have_stable_group_order() {
        let simulation = GameSimulation::new(5);
        let surface = SurfaceRenderSnapshot::new(
            simulation.default_surface_id(),
            GridSize::new(2, 1),
            vec![
                crate::world::render_snapshot::TerrainRenderCell {
                    coord: CellCoord::new(0, 0),
                    kind: TerrainKind::Water,
                    variant: 0,
                },
                crate::world::render_snapshot::TerrainRenderCell {
                    coord: CellCoord::new(1, 0),
                    kind: TerrainKind::Grass,
                    variant: 0,
                },
            ],
        );
        assert_eq!(
            terrain_chunk_geometry(&surface, ChunkCoord { x: 0, y: 0 })
                .surfaces
                .iter()
                .map(|surface| surface.kind)
                .collect::<Vec<_>>(),
            vec![TerrainKind::Grass, TerrainKind::Water]
        );

        let roads = DynamicRenderSnapshot {
            completed_roads: vec![road_cell(RoadTier::Flagstone, 0)],
            planned_roads: vec![road_cell(RoadTier::DirtPath, 0)],
            ..Default::default()
        };
        assert_eq!(
            road_chunk_geometry(&roads, ChunkCoord { x: 0, y: 0 })
                .surfaces
                .iter()
                .map(|surface| surface.key)
                .collect::<Vec<_>>(),
            vec![
                RoadSurfaceKey {
                    tier: RoadTier::Flagstone,
                    state: RoadRenderState::Completed,
                },
                RoadSurfaceKey {
                    tier: RoadTier::DirtPath,
                    state: RoadRenderState::Planned,
                },
            ]
        );
        assert_eq!(road_chunk_coords(&roads), vec![ChunkCoord { x: 0, y: 0 }]);
    }

    #[test]
    fn cross_boundary_road_change_marks_both_affected_chunks() {
        let mut previous_world = World::new();
        previous_world.spawn(Road {
            coord: CellCoord::new(31, 4),
            tier: RoadTier::DirtPath,
        });
        let mut current_world = World::new();
        current_world.spawn(Road {
            coord: CellCoord::new(31, 4),
            tier: RoadTier::DirtPath,
        });
        current_world.spawn(Road {
            coord: CellCoord::new(32, 4),
            tier: RoadTier::DirtPath,
        });

        let previous = DynamicRenderSnapshot::from_world(&previous_world);
        let current = DynamicRenderSnapshot::from_world(&current_world);
        assert_eq!(
            affected_road_chunks(&previous, &current),
            vec![ChunkCoord { x: 0, y: 0 }, ChunkCoord { x: 1, y: 0 }]
        );
    }

    fn road_cell(tier: RoadTier, connectivity_mask: u8) -> RoadRenderCell {
        RoadRenderCell {
            entity: Entity::PLACEHOLDER,
            coord: CellCoord::new(0, 0),
            tier,
            connectivity_mask,
        }
    }
}
