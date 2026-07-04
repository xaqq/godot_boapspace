use bevy_ecs::prelude::*;
use game_engine::grid::Grid;
use game_engine::resources::GameResources;

pub struct GameState {
    pub world: World,
}

impl GameState {
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Grid::new(256, 256));
        world.insert_resource(GameResources::default());
        Self { world }
    }

    pub fn tick(&mut self, _delta: f32) {
        // Run ECS systems here in the future
    }
}
