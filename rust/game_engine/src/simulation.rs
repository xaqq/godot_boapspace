use crate::components::{Terrain, TerrainKind, Tile, TileDisplay, TileDisplayEntry};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::resource_nodes::spawn_initial_resource_nodes;
use crate::resources::GameResources;
use crate::systems::build_surface_schedule;
use crate::tile::{spawn_initial_tiles, TileIndex};
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
            .run_system_once(spawn_initial_tiles)
            .expect("initial tile spawn system should run");
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

    pub fn tile_terrain_at(&self, surface_id: SurfaceId, coord: CellCoord) -> Option<TerrainKind> {
        tile_terrain_at(self.surface(surface_id)?, coord)
    }

    pub fn tile_display_at(&self, surface_id: SurfaceId, coord: CellCoord) -> Option<TileDisplay> {
        self.tile_terrain_at(surface_id, coord)
            .map(TileDisplay::from)
    }

    pub fn tile_display_entries(&self, surface_id: SurfaceId) -> Option<Vec<TileDisplayEntry>> {
        tile_display_entries(self.surface(surface_id)?)
    }

    pub fn with_surface_world<R>(
        &self,
        surface_id: SurfaceId,
        f: impl FnOnce(&World) -> R,
    ) -> Option<R> {
        Some(f(&self.surface(surface_id)?.world))
    }

    fn surface(&self, surface_id: SurfaceId) -> Option<&SurfaceRuntime> {
        self.surfaces.get(surface_id.index())
    }
}

fn tile_terrain_at(surface: &SurfaceRuntime, coord: CellCoord) -> Option<TerrainKind> {
    let index = surface.world.get_resource::<TileIndex>()?;
    let entity = index.get(coord)?;
    surface.world.get::<Tile>(entity)?;

    Some(surface.world.get::<Terrain>(entity)?.kind)
}

fn tile_display_entries(surface: &SurfaceRuntime) -> Option<Vec<TileDisplayEntry>> {
    let index = surface.world.get_resource::<TileIndex>()?;
    Some(
        index
            .iter()
            .filter_map(|(coord, entity)| {
                surface.world.get::<Tile>(entity)?;
                let terrain = surface.world.get::<Terrain>(entity)?;
                Some(TileDisplayEntry {
                    coord,
                    display: TileDisplay::from(terrain.kind),
                })
            })
            .collect(),
    )
}

impl Default for GameSimulation {
    fn default() -> Self {
        Self::new()
    }
}
