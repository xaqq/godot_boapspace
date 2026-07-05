use crate::components::{Terrain, TerrainKind, Tile, TilePosition};
use crate::grid::Grid;
use bevy_ecs::prelude::*;

pub fn spawn_initial_tiles(mut commands: Commands, grid: Res<Grid>) {
    for coord in grid.size().iter_coords() {
        commands.spawn((
            Tile,
            TilePosition { coord },
            Terrain::new(TerrainKind::Grass),
        ));
    }
}
