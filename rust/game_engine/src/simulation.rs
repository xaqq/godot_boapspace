use crate::grid::{CellCoord, CellType, Grid, GridSize};
use crate::resource_nodes::spawn_initial_resource_nodes;
use crate::resources::{GameResources, ResourceKind};
use crate::systems::build_surface_schedule;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::Schedule;
use bevy_ecs::system::RunSystemOnce;

pub const DEFAULT_GRID_SIZE: GridSize = GridSize::new(256, 256);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(usize);

impl SurfaceId {
    pub const fn index(self) -> usize {
        self.0
    }
}

struct SurfaceRuntime {
    world: World,
    schedule: Schedule,
}

impl SurfaceRuntime {
    fn new(size: GridSize) -> Self {
        let mut world = World::new();
        world.insert_resource(Grid::new(size.width(), size.height()));
        world.insert_resource(GameResources::default());
        world
            .run_system_once(spawn_initial_resource_nodes)
            .expect("initial resource node spawn system should run");

        Self {
            world,
            schedule: build_surface_schedule(),
        }
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

    fn tick(&mut self) {
        self.schedule.run(&mut self.world);
    }
}

pub struct GameSimulation {
    surfaces: Vec<SurfaceRuntime>,
    default_surface: SurfaceId,
}

impl GameSimulation {
    pub fn new() -> Self {
        let default_surface = SurfaceRuntime::new(DEFAULT_GRID_SIZE);

        Self {
            surfaces: vec![default_surface],
            default_surface: SurfaceId(0),
        }
    }

    pub fn create_surface(&mut self, size: GridSize) -> SurfaceId {
        let surface_id = SurfaceId(self.surfaces.len());
        self.surfaces.push(SurfaceRuntime::new(size));
        surface_id
    }

    pub fn default_surface_id(&self) -> SurfaceId {
        self.default_surface
    }

    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    pub fn surface_id_at(&self, index: usize) -> Option<SurfaceId> {
        (index < self.surfaces.len()).then_some(SurfaceId(index))
    }

    pub fn tick(&mut self, _delta: f32) {
        for surface in &mut self.surfaces {
            surface.tick();
        }
    }

    pub fn grid_size(&self, surface_id: SurfaceId) -> Option<GridSize> {
        Some(self.surface(surface_id)?.grid().size())
    }

    pub fn cell_type(&self, surface_id: SurfaceId, coord: CellCoord) -> Option<CellType> {
        self.surface(surface_id)?.grid().get(coord)
    }

    pub fn resource_amount(&self, surface_id: SurfaceId, kind: ResourceKind) -> Option<u32> {
        Some(self.surface(surface_id)?.resources().get(kind))
    }

    pub fn add_resource(
        &mut self,
        surface_id: SurfaceId,
        kind: ResourceKind,
        amount: u32,
    ) -> Option<bool> {
        Some(
            self.surface_mut(surface_id)?
                .resources_mut()
                .add(kind, amount),
        )
    }

    pub fn with_surface_world<R>(
        &self,
        surface_id: SurfaceId,
        f: impl FnOnce(&World) -> R,
    ) -> Option<R> {
        Some(f(&self.surface(surface_id)?.world))
    }

    pub fn with_surface_world_mut<R>(
        &mut self,
        surface_id: SurfaceId,
        f: impl FnOnce(&mut World) -> R,
    ) -> Option<R> {
        Some(f(&mut self.surface_mut(surface_id)?.world))
    }

    fn surface(&self, surface_id: SurfaceId) -> Option<&SurfaceRuntime> {
        self.surfaces.get(surface_id.index())
    }

    fn surface_mut(&mut self, surface_id: SurfaceId) -> Option<&mut SurfaceRuntime> {
        self.surfaces.get_mut(surface_id.index())
    }
}

impl Default for GameSimulation {
    fn default() -> Self {
        Self::new()
    }
}
