use bevy_ecs::prelude::*;
use game_engine::grid::{CellCoord, CellType, Grid, GridSize};
use game_engine::resources::{GameResources, ResourceKind, ResourceSnapshot};

const DEFAULT_GRID_SIZE: GridSize = GridSize::new(256, 256);

pub struct GameState {
    world: World,
}

impl GameState {
    pub fn new() -> Self {
        let mut world = World::new();
        world
            .insert_resource(Grid::try_new(DEFAULT_GRID_SIZE).expect("default grid size is valid"));
        world.insert_resource(GameResources::default());
        Self { world }
    }

    pub fn grid_size(&self) -> GridSize {
        self.grid().size()
    }

    pub fn cell_type(&self, coord: CellCoord) -> Option<CellType> {
        self.grid().get(coord)
    }

    pub fn resource_amount(&self, kind: ResourceKind) -> u32 {
        self.resources().get(kind)
    }

    pub fn resource_snapshot(&self) -> ResourceSnapshot {
        self.resources().snapshot()
    }

    pub fn add_resource(&mut self, kind: ResourceKind, amount: u32) -> bool {
        self.resources_mut().add(kind, amount)
    }

    pub fn tick(&mut self, _delta: f32) {
        // Run ECS systems here in the future
    }

    fn grid(&self) -> &Grid {
        self.world.resource::<Grid>()
    }

    fn resources(&self) -> &GameResources {
        self.world.resource::<GameResources>()
    }

    fn resources_mut(&mut self) -> Mut<'_, GameResources> {
        self.world.resource_mut::<GameResources>()
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}
