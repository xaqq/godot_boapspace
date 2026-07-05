use crate::components::{Terrain, TerrainKind, Tile, TilePosition};
use crate::grid::{CellCoord, Grid, GridSize};
use bevy_ecs::prelude::*;

const TERRAIN_PATCH_SIZE: i32 = 8;

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

pub fn spawn_initial_tiles(mut commands: Commands, grid: Res<Grid>) {
    let size = grid.size();
    let mut tile_index = TileIndex::new(size);

    for coord in size.iter_coords() {
        let terrain = terrain_kind_at(size, coord);
        let entity = commands
            .spawn(TileBundle::new_with_terrain(coord, terrain))
            .id();
        debug_assert!(tile_index.set(coord, entity));
    }

    commands.insert_resource(tile_index);
}

pub fn terrain_kind_at(size: GridSize, coord: CellCoord) -> TerrainKind {
    let patch_coord = CellCoord::new(
        coord.x().div_euclid(TERRAIN_PATCH_SIZE),
        coord.y().div_euclid(TERRAIN_PATCH_SIZE),
    );

    match terrain_hash(size, patch_coord) % 100 {
        0..=7 => TerrainKind::Water,
        8..=19 => TerrainKind::Sand,
        20..=39 => TerrainKind::Dirt,
        _ => TerrainKind::Grass,
    }
}

fn terrain_hash(size: GridSize, coord: CellCoord) -> u64 {
    let mut value = 0x517c_c1b7_2722_0a95_u64;
    value ^= (size.width() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value = value.rotate_left(23);
    value ^= (size.height() as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = value.rotate_left(29);
    value ^= (coord.x() as i64 as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    value = value.rotate_left(31);
    value ^= (coord.y() as i64 as u64).wrapping_mul(0xd6e8_feb8_6659_fd93);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
