use crate::components::{Terrain, TerrainKind, Tile, TilePosition};
use crate::grid::{CellCoord, Grid, GridSize};
use bevy_ecs::prelude::*;

const WATER_PERCENT: usize = 8;
const SAND_PERCENT: usize = 12;
const DIRT_PERCENT: usize = 20;
const NOISE_CHANNEL_ELEVATION: u64 = 0x6a09_e667_f3bc_c909;
const NOISE_CHANNEL_SOIL: u64 = 0xbb67_ae85_84ca_a73b;
const NOISE_OCTAVES: [(i32, u64); 3] = [(32, 4), (16, 2), (8, 1)];
const INTERPOLATION_SCALE: u64 = 1 << 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub(crate) struct SurfaceGeneration {
    seed: u64,
    protect_start_area: bool,
}

impl SurfaceGeneration {
    pub(crate) const fn new(seed: u64, protect_start_area: bool) -> Self {
        Self {
            seed,
            protect_start_area,
        }
    }

    pub(crate) const fn seed(self) -> u64 {
        self.seed
    }

    pub(crate) fn protects(self, size: GridSize, coord: CellCoord) -> bool {
        self.protect_start_area && start_area_contains(size, coord)
    }
}

#[derive(Debug, Clone, Resource)]
pub struct TileIndex {
    size: GridSize,
    entities: Vec<Option<Entity>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Bundle)]
pub struct TileBundle {
    tile: Tile,
    position: TilePosition,
    terrain: Terrain,
}

impl TileBundle {
    pub const fn new(coord: CellCoord) -> Self {
        Self::new_with_terrain(coord, TerrainKind::Grass)
    }

    pub const fn new_with_terrain(coord: CellCoord, terrain: TerrainKind) -> Self {
        Self {
            tile: Tile,
            position: TilePosition { coord },
            terrain: Terrain::new(terrain),
        }
    }
}

impl TileIndex {
    pub fn new(size: GridSize) -> Self {
        let cell_count = size
            .cell_count()
            .expect("grid dimensions should fit in addressable memory");

        Self {
            size,
            entities: vec![None; cell_count],
        }
    }

    pub const fn size(&self) -> GridSize {
        self.size
    }

    pub fn len(&self) -> usize {
        self.entities
            .iter()
            .filter(|entity| entity.is_some())
            .count()
    }

    pub fn get(&self, coord: CellCoord) -> Option<Entity> {
        self.index(coord).and_then(|index| self.entities[index])
    }

    pub fn set(&mut self, coord: CellCoord, entity: Entity) -> bool {
        let Some(index) = self.index(coord) else {
            return false;
        };

        self.entities[index] = Some(entity);
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = (CellCoord, Entity)> + '_ {
        self.size
            .iter_coords()
            .filter_map(move |coord| self.get(coord).map(|entity| (coord, entity)))
    }

    fn index(&self, coord: CellCoord) -> Option<usize> {
        if !self.size.contains(coord) {
            return None;
        }

        let x = usize::try_from(coord.x()).ok()?;
        let y = usize::try_from(coord.y()).ok()?;

        y.checked_mul(self.size.width())?.checked_add(x)
    }
}

pub(crate) fn spawn_initial_tiles(
    mut commands: Commands,
    grid: Res<Grid>,
    generation: Res<SurfaceGeneration>,
) {
    let size = grid.size();
    let mut tile_index = TileIndex::new(size);

    for (coord, terrain) in generate_terrain(size, *generation) {
        let entity = commands
            .spawn(TileBundle::new_with_terrain(coord, terrain))
            .id();
        let inserted = tile_index.set(coord, entity);
        debug_assert!(inserted);
    }

    commands.insert_resource(tile_index);
}

fn generate_terrain(
    size: GridSize,
    generation: SurfaceGeneration,
) -> Vec<(CellCoord, TerrainKind)> {
    let coords = size.iter_coords().collect::<Vec<_>>();
    let cell_count = coords.len();
    let mut terrain = vec![TerrainKind::Grass; cell_count];

    let mut elevation = coords
        .iter()
        .copied()
        .enumerate()
        .map(|(index, coord)| {
            (
                fractal_noise(generation.seed(), coord, NOISE_CHANNEL_ELEVATION),
                coord,
                index,
            )
        })
        .collect::<Vec<_>>();
    elevation.sort_unstable_by_key(|(score, coord, _)| (*score, coord.y(), coord.x()));

    let water_count = cell_count.saturating_mul(WATER_PERCENT) / 100;
    let sand_count = cell_count.saturating_mul(SAND_PERCENT) / 100;
    for &(_, _, index) in elevation.iter().take(water_count) {
        terrain[index] = TerrainKind::Water;
    }
    for &(_, _, index) in elevation.iter().skip(water_count).take(sand_count) {
        terrain[index] = TerrainKind::Sand;
    }

    let dirt_count = cell_count.saturating_mul(DIRT_PERCENT) / 100;
    let mut soil = elevation
        .iter()
        .skip(water_count + sand_count)
        .map(|&(_, coord, index)| {
            (
                fractal_noise(generation.seed(), coord, NOISE_CHANNEL_SOIL),
                coord,
                index,
            )
        })
        .collect::<Vec<_>>();
    soil.sort_unstable_by_key(|(score, coord, _)| (*score, coord.y(), coord.x()));
    for &(_, _, index) in soil.iter().take(dirt_count) {
        terrain[index] = TerrainKind::Dirt;
    }

    coords
        .into_iter()
        .zip(terrain)
        .map(|(coord, terrain)| {
            let terrain = if generation.protects(size, coord) {
                TerrainKind::Grass
            } else {
                terrain
            };
            (coord, terrain)
        })
        .collect()
}

fn fractal_noise(seed: u64, coord: CellCoord, channel: u64) -> u64 {
    NOISE_OCTAVES
        .iter()
        .enumerate()
        .map(|(octave, &(feature_size, weight))| {
            value_noise(
                seed ^ channel ^ (octave as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
                coord,
                feature_size,
            ) * weight
        })
        .sum()
}

fn value_noise(seed: u64, coord: CellCoord, feature_size: i32) -> u64 {
    let lattice_x = coord.x().div_euclid(feature_size);
    let lattice_y = coord.y().div_euclid(feature_size);
    let x_fraction = smooth_fraction(coord.x().rem_euclid(feature_size), feature_size);
    let y_fraction = smooth_fraction(coord.y().rem_euclid(feature_size), feature_size);

    let north_west = lattice_hash(seed, lattice_x, lattice_y) >> 32;
    let north_east = lattice_hash(seed, lattice_x + 1, lattice_y) >> 32;
    let south_west = lattice_hash(seed, lattice_x, lattice_y + 1) >> 32;
    let south_east = lattice_hash(seed, lattice_x + 1, lattice_y + 1) >> 32;
    let north = interpolate(north_west, north_east, x_fraction);
    let south = interpolate(south_west, south_east, x_fraction);
    interpolate(north, south, y_fraction)
}

fn smooth_fraction(numerator: i32, denominator: i32) -> u64 {
    let fraction = (numerator as u64 * INTERPOLATION_SCALE) / denominator as u64;
    let squared = fraction * fraction / INTERPOLATION_SCALE;
    squared * (3 * INTERPOLATION_SCALE - 2 * fraction) / INTERPOLATION_SCALE
}

fn interpolate(start: u64, end: u64, fraction: u64) -> u64 {
    (start * (INTERPOLATION_SCALE - fraction) + end * fraction) / INTERPOLATION_SCALE
}

pub(crate) fn generation_hash(seed: u64, coord: CellCoord, domain: u64) -> u64 {
    lattice_hash(seed ^ domain, coord.x(), coord.y())
}

fn lattice_hash(seed: u64, x: i32, y: i32) -> u64 {
    let mut value = seed ^ 0x517c_c1b7_2722_0a95;
    value ^= (x as i64 as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    value = value.rotate_left(31);
    value ^= (y as i64 as u64).wrapping_mul(0xd6e8_feb8_6659_fd93);
    mix_hash(value)
}

pub(crate) fn mix_hash(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn start_area_contains(size: GridSize, coord: CellCoord) -> bool {
    if size.width() == 0 || size.height() == 0 {
        return false;
    }

    let Some(center) = CellCoord::from_usize(size.width() / 2, size.height() / 2) else {
        return false;
    };
    (coord.x() - center.x()).abs() <= 1 && (coord.y() - center.y()).abs() <= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED: u64 = 0x1234_5678_9abc_def0;

    #[test]
    fn test_tile_index_set_get() {
        let mut world = World::new();
        let entity = world.spawn(TileBundle::new(CellCoord::new(1, 1))).id();
        let mut index = TileIndex::new(GridSize::new(3, 3));

        assert!(index.set(CellCoord::new(1, 1), entity));
        assert_eq!(index.get(CellCoord::new(1, 1)), Some(entity));
        assert_eq!(index.get(CellCoord::new(3, 0)), None);
    }

    #[test]
    fn test_tile_index_rejects_out_of_bounds_set() {
        let mut world = World::new();
        let entity = world.spawn(TileBundle::new(CellCoord::new(3, 0))).id();
        let mut index = TileIndex::new(GridSize::new(3, 3));

        assert!(!index.set(CellCoord::new(3, 0), entity));
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn terrain_generation_is_deterministic_and_keeps_target_distribution() {
        let size = GridSize::new(100, 100);
        let generation = SurfaceGeneration::new(TEST_SEED, false);
        let first = generate_terrain(size, generation);
        let second = generate_terrain(size, generation);

        assert_eq!(first, second);
        for (kind, expected) in [
            (TerrainKind::Water, 800),
            (TerrainKind::Sand, 1_200),
            (TerrainKind::Dirt, 2_000),
            (TerrainKind::Grass, 6_000),
        ] {
            assert_eq!(
                first.iter().filter(|(_, terrain)| *terrain == kind).count(),
                expected
            );
        }
    }

    #[test]
    fn terrain_generation_does_not_repeat_uniform_aligned_eight_tile_patches() {
        let size = GridSize::new(64, 64);
        let terrain = generate_terrain(size, SurfaceGeneration::new(TEST_SEED, false));

        let every_patch_is_uniform = (0..8).all(|patch_y| {
            (0..8).all(|patch_x| {
                let first_index = patch_y * 8 * size.width() + patch_x * 8;
                let first = terrain[first_index].1;
                (0..8).all(|offset_y| {
                    (0..8).all(|offset_x| {
                        let index =
                            (patch_y * 8 + offset_y) * size.width() + patch_x * 8 + offset_x;
                        terrain[index].1 == first
                    })
                })
            })
        });

        assert!(!every_patch_is_uniform);
    }

    #[test]
    fn protected_start_area_is_grass_and_clipped_to_the_surface() {
        for size in [GridSize::new(256, 256), GridSize::new(2, 2)] {
            let generation = SurfaceGeneration::new(TEST_SEED, true);
            let terrain = generate_terrain(size, generation);

            for (coord, kind) in terrain {
                if generation.protects(size, coord) {
                    assert_eq!(kind, TerrainKind::Grass);
                }
            }
        }
    }
}
