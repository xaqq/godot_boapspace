use crate::components::{Terrain, TerrainKind, Tile, TilePosition};
use crate::grid::{CellCoord, Grid, GridSize};
use bevy_ecs::prelude::*;

#[derive(Debug, Clone, Resource)]
pub struct TileIndex {
    size: GridSize,
    entities: Vec<Option<Entity>>,
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
        let entity = commands
            .spawn((
                Tile,
                TilePosition { coord },
                Terrain::new(TerrainKind::Grass),
            ))
            .id();
        debug_assert!(tile_index.set(coord, entity));
    }

    commands.insert_resource(tile_index);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_index_set_get() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Tile,
                TilePosition {
                    coord: CellCoord::new(1, 1),
                },
                Terrain::new(TerrainKind::Grass),
            ))
            .id();
        let mut index = TileIndex::new(GridSize::new(3, 3));

        assert!(index.set(CellCoord::new(1, 1), entity));
        assert_eq!(index.get(CellCoord::new(1, 1)), Some(entity));
        assert_eq!(index.get(CellCoord::new(3, 0)), None);
    }

    #[test]
    fn test_tile_index_rejects_out_of_bounds_set() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Tile,
                TilePosition {
                    coord: CellCoord::new(3, 0),
                },
                Terrain::new(TerrainKind::Grass),
            ))
            .id();
        let mut index = TileIndex::new(GridSize::new(3, 3));

        assert!(!index.set(CellCoord::new(3, 0), entity));
        assert_eq!(index.len(), 0);
    }
}
