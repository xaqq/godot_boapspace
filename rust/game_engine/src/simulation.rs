use crate::buildings::{
    place_building_blueprint, validate_building_blueprint_placement, BuildingFootprint,
    BuildingKind, BuildingPlacementError,
};
use crate::components::{Terrain, TerrainKind, Tile};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::npcs::{spawn_initial_default_npc, WorldDateTime, DEFAULT_WORLD_DATE_TIME_DAY};
use crate::resource_nodes::spawn_initial_resource_nodes;
use crate::systems::build_surface_schedule;
use crate::tile::{spawn_initial_tiles, TileIndex};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::Schedule;
use bevy_ecs::system::RunSystemOnce;
use std::time::Duration;

pub const DEFAULT_GRID_SIZE: GridSize = GridSize::new(256, 256);
pub const SIMULATION_TICK_SECONDS: u64 = 10 * 60;
const SIMULATION_TICK_DURATION: Duration = Duration::from_secs(SIMULATION_TICK_SECONDS);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(usize);

impl SurfaceId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceLookupError {
    IndexOutOfRange { index: usize, surface_count: usize },
}

struct SurfaceRuntime {
    world: World,
    schedule: Schedule,
}

impl SurfaceRuntime {
    fn new(size: GridSize, spawn_default_npc: bool, world_date_time: WorldDateTime) -> Self {
        let mut world = World::new();
        world.insert_resource(Grid::new(size.width(), size.height()));
        world.insert_resource(world_date_time);
        world
            .run_system_once(spawn_initial_tiles)
            .expect("initial tile spawn system should run");
        world
            .run_system_once(spawn_initial_resource_nodes)
            .expect("initial resource node spawn system should run");
        if spawn_default_npc {
            world
                .run_system_once(spawn_initial_default_npc)
                .expect("initial NPC spawn system should run");
        }

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

    fn set_world_date_time(&mut self, world_date_time: WorldDateTime) {
        if let Some(mut resource) = self.world.get_resource_mut::<WorldDateTime>() {
            *resource = world_date_time;
        } else {
            self.world.insert_resource(world_date_time);
        }
    }
}

pub struct GameSimulation {
    surfaces: Vec<SurfaceRuntime>,
    default_surface: SurfaceId,
    world_date_time: WorldDateTime,
    playing: bool,
}

impl GameSimulation {
    pub fn new() -> Self {
        let world_date_time = WorldDateTime::from_day(DEFAULT_WORLD_DATE_TIME_DAY);
        let default_surface = SurfaceRuntime::new(DEFAULT_GRID_SIZE, true, world_date_time);

        Self {
            surfaces: vec![default_surface],
            default_surface: SurfaceId(0),
            world_date_time,
            playing: true,
        }
    }

    pub fn create_surface(&mut self, size: GridSize) -> SurfaceId {
        let surface_id = SurfaceId(self.surfaces.len());
        self.surfaces
            .push(SurfaceRuntime::new(size, false, self.world_date_time));
        surface_id
    }

    pub fn default_surface_id(&self) -> SurfaceId {
        self.default_surface
    }

    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    pub fn surface_id_at(&self, index: usize) -> Result<SurfaceId, SurfaceLookupError> {
        if index < self.surfaces.len() {
            Ok(SurfaceId(index))
        } else {
            Err(SurfaceLookupError::IndexOutOfRange {
                index,
                surface_count: self.surfaces.len(),
            })
        }
    }

    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn toggle_playing(&mut self) {
        self.playing = !self.playing;
    }

    pub const fn world_date_time(&self) -> WorldDateTime {
        self.world_date_time
    }

    pub fn tick(&mut self, _delta: f32) {
        if !self.playing {
            return;
        }

        self.world_date_time.advance_by(SIMULATION_TICK_DURATION);
        for surface in &mut self.surfaces {
            surface.tick();
            surface.set_world_date_time(self.world_date_time);
        }
    }

    pub fn grid_size(&self, surface_id: SurfaceId) -> GridSize {
        self.surface(surface_id).grid().size()
    }

    pub fn tile_terrain_at(&self, surface_id: SurfaceId, coord: CellCoord) -> Option<TerrainKind> {
        tile_terrain_at(self.surface(surface_id), coord)
    }

    pub fn tile_coords(&self, surface_id: SurfaceId) -> Vec<CellCoord> {
        tile_coords(self.surface(surface_id))
    }

    pub fn with_surface_world<R>(&self, surface_id: SurfaceId, f: impl FnOnce(&World) -> R) -> R {
        f(&self.surface(surface_id).world)
    }

    pub fn place_building_blueprint(
        &mut self,
        surface_id: SurfaceId,
        kind: BuildingKind,
        origin: CellCoord,
    ) -> Result<Entity, BuildingPlacementError> {
        let surface = self.surface_mut(surface_id);
        place_building_blueprint(&mut surface.world, kind, origin)
    }

    pub fn validate_building_blueprint_placement(
        &self,
        surface_id: SurfaceId,
        kind: BuildingKind,
        origin: CellCoord,
    ) -> Result<BuildingFootprint, BuildingPlacementError> {
        let surface = self.surface(surface_id);
        validate_building_blueprint_placement(&surface.world, kind, origin)
    }

    fn surface(&self, surface_id: SurfaceId) -> &SurfaceRuntime {
        self.surfaces
            .get(surface_id.index())
            .expect("surface id should have been issued by this simulation")
    }

    fn surface_mut(&mut self, surface_id: SurfaceId) -> &mut SurfaceRuntime {
        self.surfaces
            .get_mut(surface_id.index())
            .expect("surface id should have been issued by this simulation")
    }
}

fn tile_terrain_at(surface: &SurfaceRuntime, coord: CellCoord) -> Option<TerrainKind> {
    let index = surface
        .world
        .get_resource::<TileIndex>()
        .expect("surface world should have a tile index");
    let entity = index.get(coord)?;
    surface
        .world
        .get::<Tile>(entity)
        .expect("tile index should reference a tile entity");

    Some(
        surface
            .world
            .get::<Terrain>(entity)
            .expect("tile entity should have terrain")
            .kind,
    )
}

fn tile_coords(surface: &SurfaceRuntime) -> Vec<CellCoord> {
    let index = surface
        .world
        .get_resource::<TileIndex>()
        .expect("surface world should have a tile index");
    index
        .iter()
        .map(|(coord, entity)| {
            surface
                .world
                .get::<Tile>(entity)
                .expect("tile index should reference a tile entity");
            coord
        })
        .collect()
}

impl Default for GameSimulation {
    fn default() -> Self {
        Self::new()
    }
}
