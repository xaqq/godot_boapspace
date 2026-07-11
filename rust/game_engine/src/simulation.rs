use crate::buildings::{
    building_name_exists, place_building_blueprint, validate_building_blueprint_placement,
    Building, BuildingActivity, BuildingFootprint, BuildingKind, BuildingName,
    BuildingNameRegistry, BuildingPlacementError, RefineryPullConfig, StorageInventory,
    StoragePullConfig, WarehouseInventory,
};
use crate::collision::{collision_flags_at, CollisionFlags};
use crate::components::{Terrain, TerrainKind, Tile};
use crate::farming::{
    place_field_blueprint, place_field_blueprints, validate_field_blueprint_placement,
    validate_field_blueprint_placement_batch, FieldPlacementBatchResult, FieldPlacementError,
    FieldPlacementPreview,
};
use crate::forestry::{
    place_tree_plot_blueprint, place_tree_plot_blueprints, validate_tree_plot_blueprint_placement,
    validate_tree_plot_blueprint_placement_batch, TreePlotPlacementBatchResult,
    TreePlotPlacementError, TreePlotPlacementPreview,
};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::logistics::cancel_work_involving_building;
use crate::npcs::{spawn_initial_default_npcs, WorldDateTime, DEFAULT_WORLD_DATE_TIME_DAY};
use crate::refining::{recipes_for_building, refinery_status, RefineryStatus, ReservationLedger};
use crate::resource_nodes::spawn_initial_resource_nodes;
use crate::resources::{resource_overview, ResourceHistory, ResourceKind, ResourceOverview};
use crate::roads::{
    place_road_blueprints, road_cell_view, validate_road_placement_batch, RoadCellView, RoadMap,
    RoadPlacementBatchResult, RoadTier,
};
use crate::systems::build_surface_schedule;
use crate::tile::{mix_hash, spawn_initial_tiles, SurfaceGeneration, TileIndex};
use crate::time::SIMULATION_TICK_DURATION;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::Schedule;
use bevy_ecs::system::RunSystemOnce;

pub const DEFAULT_GRID_SIZE: GridSize = GridSize::new(256, 256);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationSpeed {
    OneX,
    TwoX,
    FourX,
    FiftyX,
    HundredX,
}

impl SimulationSpeed {
    pub const fn multiplier(self) -> u32 {
        match self {
            Self::OneX => 1,
            Self::TwoX => 2,
            Self::FourX => 4,
            Self::FiftyX => 50,
            Self::HundredX => 100,
        }
    }

    pub const fn from_multiplier(multiplier: u32) -> Option<Self> {
        match multiplier {
            1 => Some(Self::OneX),
            2 => Some(Self::TwoX),
            4 => Some(Self::FourX),
            50 => Some(Self::FiftyX),
            100 => Some(Self::HundredX),
            _ => None,
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarehouseFilterError {
    MissingEntity,
    NotCompletedWarehouse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BuildingTarget {
    surface_id: SurfaceId,
    entity: Entity,
}

impl BuildingTarget {
    pub const fn new(surface_id: SurfaceId, entity: Entity) -> Self {
        Self { surface_id, entity }
    }

    pub const fn surface_id(self) -> SurfaceId {
        self.surface_id
    }

    pub const fn entity(self) -> Entity {
        self.entity
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingCommandError {
    WrongSurface,
    MissingEntity,
    NotBuilding,
    BlueprintIneligible,
    UnsupportedBuilding,
    UnsupportedResource,
    InvalidName,
    DuplicateName,
}

struct SurfaceRuntime {
    world: World,
    schedule: Schedule,
}

impl SurfaceRuntime {
    fn new(
        size: GridSize,
        generation_seed: u64,
        spawn_default_npc: bool,
        world_date_time: WorldDateTime,
    ) -> Self {
        let mut world = World::new();
        world.insert_resource(Grid::new(size.width(), size.height()));
        world.insert_resource(SurfaceGeneration::new(generation_seed, spawn_default_npc));
        world.insert_resource(world_date_time);
        world.insert_resource(ReservationLedger::default());
        world.insert_resource(BuildingNameRegistry::default());
        world.insert_resource(RoadMap::default());
        world
            .run_system_once(spawn_initial_tiles)
            .expect("initial tile spawn system should run");
        world
            .run_system_once(spawn_initial_resource_nodes)
            .expect("initial resource node spawn system should run");
        if spawn_default_npc {
            world
                .run_system_once(spawn_initial_default_npcs)
                .expect("initial NPC spawn system should run");
        }

        let initial_usable = resource_overview(&mut world).usable();
        world.insert_resource(ResourceHistory::new(world_date_time.day(), initial_usable));

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
        let day = self.world.resource::<WorldDateTime>().day();
        if self
            .world
            .resource::<ResourceHistory>()
            .samples()
            .last()
            .is_some_and(|sample| sample.day() >= day)
        {
            return;
        }
        let usable = resource_overview(&mut self.world).usable();
        self.world
            .resource_mut::<ResourceHistory>()
            .record_day(day, usable);
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
    generation_seed: u64,
    world_date_time: WorldDateTime,
    playing: bool,
    simulation_speed: SimulationSpeed,
}

impl GameSimulation {
    pub fn new(generation_seed: u64) -> Self {
        let world_date_time = WorldDateTime::from_day(DEFAULT_WORLD_DATE_TIME_DAY);
        let default_surface_id = SurfaceId(0);
        let default_surface = SurfaceRuntime::new(
            DEFAULT_GRID_SIZE,
            surface_generation_seed(generation_seed, default_surface_id),
            true,
            world_date_time,
        );

        Self {
            surfaces: vec![default_surface],
            default_surface: default_surface_id,
            generation_seed,
            world_date_time,
            playing: true,
            simulation_speed: SimulationSpeed::OneX,
        }
    }

    pub fn create_surface(&mut self, size: GridSize) -> SurfaceId {
        let surface_id = SurfaceId(self.surfaces.len());
        self.surfaces.push(SurfaceRuntime::new(
            size,
            surface_generation_seed(self.generation_seed, surface_id),
            false,
            self.world_date_time,
        ));
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

    pub const fn simulation_speed(&self) -> SimulationSpeed {
        self.simulation_speed
    }

    pub fn set_simulation_speed(&mut self, simulation_speed: SimulationSpeed) {
        self.simulation_speed = simulation_speed;
    }

    pub fn tick(&mut self) {
        if !self.playing {
            return;
        }

        for _ in 0..self.simulation_speed.multiplier() {
            self.run_fixed_tick();
        }
    }

    fn run_fixed_tick(&mut self) {
        self.world_date_time.advance_by(SIMULATION_TICK_DURATION);
        for surface in &mut self.surfaces {
            surface.set_world_date_time(self.world_date_time);
            surface.tick();
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

    pub fn collision_flags_at(
        &self,
        surface_id: SurfaceId,
        coord: CellCoord,
    ) -> Option<CollisionFlags> {
        collision_flags_at(&self.surface(surface_id).world, coord)
    }

    pub fn with_surface_world<R>(&self, surface_id: SurfaceId, f: impl FnOnce(&World) -> R) -> R {
        f(&self.surface(surface_id).world)
    }

    pub fn with_surface_resource_overview<R>(
        &mut self,
        surface_id: SurfaceId,
        f: impl FnOnce(ResourceOverview, &World) -> R,
    ) -> R {
        let surface = self.surface_mut(surface_id);
        let overview = resource_overview(&mut surface.world);
        f(overview, &surface.world)
    }

    pub fn resource_overview(&mut self, surface_id: SurfaceId) -> ResourceOverview {
        resource_overview(&mut self.surface_mut(surface_id).world)
    }

    pub fn resource_history(&self, surface_id: SurfaceId) -> &ResourceHistory {
        self.surface(surface_id).world.resource::<ResourceHistory>()
    }

    pub fn refinery_status(
        &self,
        surface_id: SurfaceId,
        refinery: Entity,
    ) -> Option<RefineryStatus> {
        refinery_status(&self.surface(surface_id).world, refinery)
    }

    pub fn building_name(
        &self,
        surface_id: SurfaceId,
        target: BuildingTarget,
    ) -> Result<&str, BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        let world = &self.surface(surface_id).world;
        validate_building_entity(world, target.entity)?;
        world
            .get::<BuildingName>(target.entity)
            .map(BuildingName::as_str)
            .ok_or(BuildingCommandError::NotBuilding)
    }

    pub fn rename_building(
        &mut self,
        surface_id: SurfaceId,
        target: BuildingTarget,
        requested_name: &str,
    ) -> Result<(), BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        let trimmed = requested_name.trim();
        if !(1..=64).contains(&trimmed.chars().count()) {
            return Err(BuildingCommandError::InvalidName);
        }
        let world = &mut self.surface_mut(surface_id).world;
        validate_building_entity(world, target.entity)?;
        if building_name_exists(world, trimmed, Some(target.entity)) {
            return Err(BuildingCommandError::DuplicateName);
        }
        let Some(mut name) = world.get_mut::<BuildingName>(target.entity) else {
            return Err(BuildingCommandError::NotBuilding);
        };
        *name = BuildingName::new(trimmed);
        Ok(())
    }

    pub fn building_active(
        &self,
        surface_id: SurfaceId,
        target: BuildingTarget,
    ) -> Result<bool, BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        let world = &self.surface(surface_id).world;
        let building = completed_building(world, target.entity)?;
        if !building.kind.is_logistics_configurable() {
            return Err(BuildingCommandError::UnsupportedBuilding);
        }
        world
            .get::<BuildingActivity>(target.entity)
            .map(|activity| activity.is_active())
            .ok_or(BuildingCommandError::UnsupportedBuilding)
    }

    pub fn set_building_active(
        &mut self,
        surface_id: SurfaceId,
        target: BuildingTarget,
        active: bool,
    ) -> Result<(), BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        let world = &mut self.surface_mut(surface_id).world;
        let building = completed_building(world, target.entity)?;
        if !building.kind.is_logistics_configurable() {
            return Err(BuildingCommandError::UnsupportedBuilding);
        }
        let Some(mut activity) = world.get_mut::<BuildingActivity>(target.entity) else {
            return Err(BuildingCommandError::UnsupportedBuilding);
        };
        activity.set_active(active);
        drop(activity);
        if !active {
            cancel_work_involving_building(world, target.entity);
        }
        Ok(())
    }

    pub fn storage_resource_allowed(
        &self,
        surface_id: SurfaceId,
        target: BuildingTarget,
        kind: ResourceKind,
    ) -> Result<bool, BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        let world = &self.surface(surface_id).world;
        completed_storage(world, target.entity)?;
        Ok(world
            .get::<StorageInventory>(target.entity)
            .expect("validated storage should have inventory")
            .is_allowed(kind))
    }

    pub fn set_storage_resource_allowed(
        &mut self,
        surface_id: SurfaceId,
        target: BuildingTarget,
        kind: ResourceKind,
        allowed: bool,
    ) -> Result<(), BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        let world = &mut self.surface_mut(surface_id).world;
        completed_storage(world, target.entity)?;
        world
            .get_mut::<StorageInventory>(target.entity)
            .expect("validated storage should have inventory")
            .set_allowed(kind, allowed);
        if !allowed && StoragePullConfig::supports(kind) {
            world
                .get_mut::<StoragePullConfig>(target.entity)
                .expect("validated storage should have pull configuration")
                .set_pulls_from_refineries(kind, false);
        }
        Ok(())
    }

    pub fn storage_pulls_from_refineries(
        &self,
        surface_id: SurfaceId,
        target: BuildingTarget,
        kind: ResourceKind,
    ) -> Result<bool, BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        if !StoragePullConfig::supports(kind) {
            return Err(BuildingCommandError::UnsupportedResource);
        }
        let world = &self.surface(surface_id).world;
        completed_storage(world, target.entity)?;
        Ok(world
            .get::<StoragePullConfig>(target.entity)
            .expect("validated storage should have pull configuration")
            .pulls_from_refineries(kind))
    }

    pub fn set_storage_pulls_from_refineries(
        &mut self,
        surface_id: SurfaceId,
        target: BuildingTarget,
        kind: ResourceKind,
        enabled: bool,
    ) -> Result<(), BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        if !StoragePullConfig::supports(kind) {
            return Err(BuildingCommandError::UnsupportedResource);
        }
        let world = &mut self.surface_mut(surface_id).world;
        completed_storage(world, target.entity)?;
        world
            .get_mut::<StoragePullConfig>(target.entity)
            .expect("validated storage should have pull configuration")
            .set_pulls_from_refineries(kind, enabled);
        if enabled {
            world
                .get_mut::<StorageInventory>(target.entity)
                .expect("validated storage should have inventory")
                .set_allowed(kind, true);
        }
        Ok(())
    }

    pub fn refinery_pulls_from_storage(
        &self,
        surface_id: SurfaceId,
        target: BuildingTarget,
        kind: ResourceKind,
    ) -> Result<bool, BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        let world = &self.surface(surface_id).world;
        completed_refinery_supporting(world, target.entity, kind)?;
        Ok(world
            .get::<RefineryPullConfig>(target.entity)
            .expect("validated refinery should have pull configuration")
            .pulls_from_storage(kind))
    }

    pub fn set_refinery_pulls_from_storage(
        &mut self,
        surface_id: SurfaceId,
        target: BuildingTarget,
        kind: ResourceKind,
        enabled: bool,
    ) -> Result<(), BuildingCommandError> {
        validate_target_surface(surface_id, target)?;
        let world = &mut self.surface_mut(surface_id).world;
        completed_refinery_supporting(world, target.entity, kind)?;
        world
            .get_mut::<RefineryPullConfig>(target.entity)
            .expect("validated refinery should have pull configuration")
            .set_pulls_from_storage(kind, enabled);
        Ok(())
    }

    pub fn warehouse_resource_allowed(
        &self,
        surface_id: SurfaceId,
        warehouse: Entity,
        kind: ResourceKind,
    ) -> Result<bool, WarehouseFilterError> {
        let world = &self.surface(surface_id).world;
        if world.get_entity(warehouse).is_err() {
            return Err(WarehouseFilterError::MissingEntity);
        }
        world
            .get::<WarehouseInventory>(warehouse)
            .map(|inventory| inventory.is_allowed(kind))
            .ok_or(WarehouseFilterError::NotCompletedWarehouse)
    }

    pub fn set_warehouse_resource_allowed(
        &mut self,
        surface_id: SurfaceId,
        warehouse: Entity,
        kind: ResourceKind,
        allowed: bool,
    ) -> Result<(), WarehouseFilterError> {
        let world = &mut self.surface_mut(surface_id).world;
        if world.get_entity(warehouse).is_err() {
            return Err(WarehouseFilterError::MissingEntity);
        }
        let Some(mut inventory) = world.get_mut::<WarehouseInventory>(warehouse) else {
            return Err(WarehouseFilterError::NotCompletedWarehouse);
        };
        inventory.set_allowed(kind, allowed);
        Ok(())
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

    pub fn place_field_blueprint(
        &mut self,
        surface_id: SurfaceId,
        farm: Entity,
        coord: CellCoord,
    ) -> Result<Entity, FieldPlacementError> {
        let surface = self.surface_mut(surface_id);
        place_field_blueprint(&mut surface.world, farm, coord)
    }

    pub fn place_field_blueprints(
        &mut self,
        surface_id: SurfaceId,
        farm: Entity,
        coords: impl IntoIterator<Item = CellCoord>,
    ) -> FieldPlacementBatchResult {
        let surface = self.surface_mut(surface_id);
        place_field_blueprints(&mut surface.world, farm, coords)
    }

    pub fn validate_field_blueprint_placement(
        &self,
        surface_id: SurfaceId,
        farm: Entity,
        coord: CellCoord,
    ) -> Result<BuildingFootprint, FieldPlacementError> {
        let surface = self.surface(surface_id);
        validate_field_blueprint_placement(&surface.world, farm, coord)
    }

    pub fn validate_field_blueprint_placement_batch(
        &self,
        surface_id: SurfaceId,
        farm: Entity,
        coords: impl IntoIterator<Item = CellCoord>,
    ) -> Vec<FieldPlacementPreview> {
        let surface = self.surface(surface_id);
        validate_field_blueprint_placement_batch(&surface.world, farm, coords)
    }

    pub fn place_tree_plot_blueprint(
        &mut self,
        surface_id: SurfaceId,
        forester_lodge: Entity,
        coord: CellCoord,
    ) -> Result<Entity, TreePlotPlacementError> {
        let surface = self.surface_mut(surface_id);
        place_tree_plot_blueprint(&mut surface.world, forester_lodge, coord)
    }

    pub fn place_tree_plot_blueprints(
        &mut self,
        surface_id: SurfaceId,
        forester_lodge: Entity,
        coords: impl IntoIterator<Item = CellCoord>,
    ) -> TreePlotPlacementBatchResult {
        let surface = self.surface_mut(surface_id);
        place_tree_plot_blueprints(&mut surface.world, forester_lodge, coords)
    }

    pub fn validate_tree_plot_blueprint_placement(
        &self,
        surface_id: SurfaceId,
        forester_lodge: Entity,
        coord: CellCoord,
    ) -> Result<BuildingFootprint, TreePlotPlacementError> {
        let surface = self.surface(surface_id);
        validate_tree_plot_blueprint_placement(&surface.world, forester_lodge, coord)
    }

    pub fn validate_tree_plot_blueprint_placement_batch(
        &self,
        surface_id: SurfaceId,
        forester_lodge: Entity,
        coords: impl IntoIterator<Item = CellCoord>,
    ) -> Vec<TreePlotPlacementPreview> {
        let surface = self.surface(surface_id);
        validate_tree_plot_blueprint_placement_batch(&surface.world, forester_lodge, coords)
    }

    pub fn validate_road_placement_batch(
        &self,
        surface_id: SurfaceId,
        tier: RoadTier,
        coords: impl IntoIterator<Item = CellCoord>,
    ) -> RoadPlacementBatchResult {
        validate_road_placement_batch(&self.surface(surface_id).world, tier, coords)
    }

    pub fn place_road_blueprints(
        &mut self,
        surface_id: SurfaceId,
        tier: RoadTier,
        coords: impl IntoIterator<Item = CellCoord>,
    ) -> Result<Vec<Entity>, RoadPlacementBatchResult> {
        place_road_blueprints(&mut self.surface_mut(surface_id).world, tier, coords)
    }

    pub fn road_cell_view(&self, surface_id: SurfaceId, coord: CellCoord) -> Option<RoadCellView> {
        road_cell_view(&self.surface(surface_id).world, coord)
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

fn validate_target_surface(
    requested_surface: SurfaceId,
    target: BuildingTarget,
) -> Result<(), BuildingCommandError> {
    if requested_surface == target.surface_id {
        Ok(())
    } else {
        Err(BuildingCommandError::WrongSurface)
    }
}

fn validate_building_entity(world: &World, entity: Entity) -> Result<(), BuildingCommandError> {
    let entity_ref = world
        .get_entity(entity)
        .map_err(|_| BuildingCommandError::MissingEntity)?;
    if entity_ref.contains::<Building>()
        || entity_ref.contains::<crate::buildings::BuildingBlueprint>()
    {
        Ok(())
    } else {
        Err(BuildingCommandError::NotBuilding)
    }
}

fn completed_building(world: &World, entity: Entity) -> Result<Building, BuildingCommandError> {
    validate_building_entity(world, entity)?;
    world
        .get::<Building>(entity)
        .copied()
        .ok_or(BuildingCommandError::BlueprintIneligible)
}

fn completed_storage(world: &World, entity: Entity) -> Result<Building, BuildingCommandError> {
    let building = completed_building(world, entity)?;
    if building.kind.is_storage() && world.get::<StorageInventory>(entity).is_some() {
        Ok(building)
    } else {
        Err(BuildingCommandError::UnsupportedBuilding)
    }
}

fn completed_refinery_supporting(
    world: &World,
    entity: Entity,
    kind: ResourceKind,
) -> Result<Building, BuildingCommandError> {
    let building = completed_building(world, entity)?;
    if !building.kind.is_refinery() || world.get::<RefineryPullConfig>(entity).is_none() {
        return Err(BuildingCommandError::UnsupportedBuilding);
    }
    if !recipes_for_building(building.kind)
        .iter()
        .any(|recipe| recipe.definition().input() == kind)
    {
        return Err(BuildingCommandError::UnsupportedResource);
    }
    Ok(building)
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

fn surface_generation_seed(generation_seed: u64, surface_id: SurfaceId) -> u64 {
    mix_hash(
        generation_seed
            ^ (surface_id.index() as u64)
                .wrapping_add(1)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Building, BuildingFootprint, BuildingKind};
    use crate::components::CarriedResource;
    use crate::forestry::{
        AiCutTreePlot, AiSeedTreePlot, Forester, ForesterLodgeInventory, TreePlotGrowth,
        TreePlotOwner, TREE_PLOT_GROWTH_TICKS,
    };
    use crate::grid::CellCoord;
    use crate::npcs::{Npc, NpcPosition};
    use crate::resources::ResourceKind;
    use crate::time::{SECONDS_PER_DAY, SIMULATION_TICK_SECONDS};
    use std::time::Duration;

    const TEST_GENERATION_SEED: u64 = 0x5eed_cafe_f00d_beef;

    #[test]
    fn surfaces_record_their_live_initial_state_on_the_creation_day() {
        let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
        let default_surface = simulation.default_surface_id();

        let initial = simulation.resource_history(default_surface).samples();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].day(), DEFAULT_WORLD_DATE_TIME_DAY);
        assert_eq!(initial[0].quantity(ResourceKind::Food), 0);

        simulation.world_date_time = WorldDateTime::from_day(42);
        let empty_surface = simulation.create_surface(GridSize::new(2, 2));
        let initial = simulation.resource_history(empty_surface).samples();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].day(), 42);
        for kind in ResourceKind::ALL {
            assert_eq!(initial[0].quantity(kind), 0);
        }
    }

    #[test]
    fn paused_simulation_does_not_record_a_day_boundary() {
        let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
        let surface = simulation.create_surface(GridSize::new(2, 2));
        simulation.world_date_time = WorldDateTime::new(Duration::from_secs(
            SECONDS_PER_DAY - SIMULATION_TICK_SECONDS,
        ));
        simulation.pause();

        simulation.tick();

        assert_eq!(simulation.resource_history(surface).samples().len(), 1);
        assert_eq!(simulation.world_date_time().day(), 0);

        simulation.play();
        simulation.tick();
        let samples = simulation.resource_history(surface).samples();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[1].day(), 1);
    }

    #[test]
    fn accelerated_ticks_record_a_crossed_day_once() {
        let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
        let surface = simulation.create_surface(GridSize::new(2, 2));
        simulation.world_date_time = WorldDateTime::new(Duration::from_secs(
            SECONDS_PER_DAY - SIMULATION_TICK_SECONDS,
        ));
        simulation.set_simulation_speed(SimulationSpeed::FourX);

        simulation.tick();

        let samples = simulation.resource_history(surface).samples();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].day(), 0);
        assert_eq!(samples[1].day(), 1);
    }

    #[test]
    fn daily_samples_are_surface_local_and_remain_immutable() {
        let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
        let first = simulation.create_surface(GridSize::new(2, 2));
        let second = simulation.create_surface(GridSize::new(2, 2));
        simulation
            .surface_mut(first)
            .world
            .spawn(CarriedResource::of(ResourceKind::Wood, 3));
        simulation
            .surface_mut(second)
            .world
            .spawn(CarriedResource::of(ResourceKind::Wood, 5));
        simulation
            .surface_mut(second)
            .world
            .spawn(CarriedResource::of(ResourceKind::Wood, 4));
        simulation.world_date_time = WorldDateTime::new(Duration::from_secs(
            SECONDS_PER_DAY - SIMULATION_TICK_SECONDS,
        ));

        simulation.tick();

        assert_eq!(
            simulation
                .resource_history(first)
                .sample_on(1)
                .unwrap()
                .quantity(ResourceKind::Wood),
            3
        );
        assert_eq!(
            simulation
                .resource_history(second)
                .sample_on(1)
                .unwrap()
                .quantity(ResourceKind::Wood),
            9
        );

        let inventory_entity = simulation
            .surface(first)
            .world
            .iter_entities()
            .find(|entity| entity.get::<CarriedResource>().is_some())
            .unwrap()
            .id();
        assert!(simulation
            .surface_mut(first)
            .world
            .get_mut::<CarriedResource>(inventory_entity)
            .unwrap()
            .add(ResourceKind::Wood, 2));

        assert_eq!(
            simulation
                .resource_overview(first)
                .usable()
                .get(ResourceKind::Wood),
            5
        );
        assert_eq!(
            simulation
                .resource_history(first)
                .sample_on(1)
                .unwrap()
                .quantity(ResourceKind::Wood),
            3
        );
        assert_eq!(
            simulation
                .resource_history(second)
                .sample_on(1)
                .unwrap()
                .quantity(ResourceKind::Wood),
            9
        );
    }

    #[test]
    fn warehouse_filters_are_validated_and_surface_local() {
        let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
        let first = simulation.create_surface(GridSize::new(8, 8));
        let second = simulation.create_surface(GridSize::new(8, 8));
        let footprint = BuildingFootprint::new(CellCoord::new(0, 0), 2, 2);
        let first_warehouse = simulation
            .surface_mut(first)
            .world
            .spawn((
                Building::new(BuildingKind::Warehouse, footprint),
                WarehouseInventory::empty(),
            ))
            .id();
        let second_warehouse = simulation
            .surface_mut(second)
            .world
            .spawn((
                Building::new(BuildingKind::Warehouse, footprint),
                WarehouseInventory::empty(),
            ))
            .id();
        let town_hall = simulation
            .surface_mut(first)
            .world
            .spawn(Building::new(BuildingKind::TownHall, footprint))
            .id();

        assert_eq!(
            simulation.set_warehouse_resource_allowed(
                first,
                first_warehouse,
                ResourceKind::Wood,
                false,
            ),
            Ok(())
        );
        assert_eq!(
            simulation.warehouse_resource_allowed(first, first_warehouse, ResourceKind::Wood),
            Ok(false)
        );
        assert_eq!(
            simulation.warehouse_resource_allowed(second, second_warehouse, ResourceKind::Wood),
            Ok(true)
        );
        assert_eq!(
            simulation.set_warehouse_resource_allowed(first, town_hall, ResourceKind::Wood, false,),
            Err(WarehouseFilterError::NotCompletedWarehouse)
        );
        assert_eq!(
            simulation.set_warehouse_resource_allowed(
                first,
                Entity::PLACEHOLDER,
                ResourceKind::Wood,
                false,
            ),
            Err(WarehouseFilterError::MissingEntity)
        );
    }

    #[test]
    fn typed_building_commands_validate_surface_names_and_pull_dependencies() {
        let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
        let first = simulation.create_surface(GridSize::new(8, 8));
        let second = simulation.create_surface(GridSize::new(8, 8));
        let footprint = BuildingFootprint::new(CellCoord::new(0, 0), 2, 2);
        let depot = simulation
            .surface_mut(first)
            .world
            .spawn((
                Building::new(BuildingKind::Depot, footprint),
                BuildingName::new("Depot #1"),
                StorageInventory::for_kind(BuildingKind::Depot),
                StoragePullConfig::default(),
                BuildingActivity::active(),
            ))
            .id();
        let other = simulation
            .surface_mut(first)
            .world
            .spawn((
                Building::new(BuildingKind::TownHall, footprint),
                BuildingName::new("TownHall #1"),
            ))
            .id();
        let target = BuildingTarget::new(first, depot);

        assert_eq!(
            simulation.rename_building(first, target, "  Main Depot  "),
            Ok(())
        );
        assert_eq!(simulation.building_name(first, target), Ok("Main Depot"));
        assert_eq!(
            simulation.rename_building(first, target, "townhall #1"),
            Err(BuildingCommandError::DuplicateName)
        );
        assert_eq!(
            simulation.rename_building(first, target, &"x".repeat(65)),
            Err(BuildingCommandError::InvalidName)
        );
        assert_eq!(
            simulation
                .set_storage_pulls_from_refineries(first, target, ResourceKind::Planks, true,),
            Ok(())
        );
        assert_eq!(
            simulation.storage_resource_allowed(first, target, ResourceKind::Planks),
            Ok(true)
        );
        assert_eq!(
            simulation.set_storage_resource_allowed(first, target, ResourceKind::Planks, false,),
            Ok(())
        );
        assert_eq!(
            simulation.storage_pulls_from_refineries(first, target, ResourceKind::Planks),
            Ok(false)
        );
        assert_eq!(
            simulation.building_name(second, target),
            Err(BuildingCommandError::WrongSurface)
        );
        assert_eq!(
            simulation.set_building_active(first, BuildingTarget::new(first, other), false,),
            Err(BuildingCommandError::UnsupportedBuilding)
        );
    }

    #[test]
    fn pause_and_speed_multiplier_apply_to_forestry_progress() {
        let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
        let surface = simulation.create_surface(GridSize::new(8, 8));
        let (seed_plot, growth_plot, cut_worker) = {
            let world = &mut simulation.surface_mut(surface).world;
            let lodge = world
                .spawn((
                    Building::new(
                        BuildingKind::ForesterLodge,
                        BuildingFootprint::new(CellCoord::new(0, 0), 3, 3),
                    ),
                    ForesterLodgeInventory::empty(),
                ))
                .id();
            let seed_plot = world
                .spawn((
                    Building::new(
                        BuildingKind::TreePlot,
                        BuildingFootprint::new(CellCoord::new(0, 3), 1, 1),
                    ),
                    TreePlotOwner::new(lodge),
                    TreePlotGrowth::seedable(),
                ))
                .id();
            let growth_plot = world
                .spawn((
                    Building::new(
                        BuildingKind::TreePlot,
                        BuildingFootprint::new(CellCoord::new(1, 3), 1, 1),
                    ),
                    TreePlotOwner::new(lodge),
                    TreePlotGrowth::growing(0),
                ))
                .id();
            let cut_plot = world
                .spawn((
                    Building::new(
                        BuildingKind::TreePlot,
                        BuildingFootprint::new(CellCoord::new(2, 3), 1, 1),
                    ),
                    TreePlotOwner::new(lodge),
                    TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS),
                ))
                .id();
            world.spawn((
                Npc,
                Forester,
                NpcPosition::new(CellCoord::new(0, 3)),
                CarriedResource::empty(),
                AiSeedTreePlot::new(seed_plot),
            ));
            let cut_worker = world
                .spawn((
                    Npc,
                    Forester,
                    NpcPosition::new(CellCoord::new(2, 3)),
                    CarriedResource::empty(),
                    AiCutTreePlot::new(cut_plot),
                ))
                .id();
            (seed_plot, growth_plot, cut_worker)
        };

        simulation.pause();
        simulation.tick();
        simulation.with_surface_world(surface, |world| {
            assert_eq!(
                world
                    .get::<TreePlotGrowth>(seed_plot)
                    .unwrap()
                    .seeding_progress_ticks(),
                0
            );
            assert_eq!(
                world
                    .get::<TreePlotGrowth>(growth_plot)
                    .unwrap()
                    .growth_ticks(),
                Some(0)
            );
            assert_eq!(
                world
                    .get::<AiCutTreePlot>(cut_worker)
                    .unwrap()
                    .progress_ticks(),
                0
            );
        });

        simulation.play();
        simulation.set_simulation_speed(SimulationSpeed::FourX);
        simulation.tick();
        simulation.with_surface_world(surface, |world| {
            assert_eq!(
                world
                    .get::<TreePlotGrowth>(seed_plot)
                    .unwrap()
                    .seeding_progress_ticks(),
                4
            );
            assert_eq!(
                world
                    .get::<TreePlotGrowth>(growth_plot)
                    .unwrap()
                    .growth_ticks(),
                Some(4)
            );
            assert_eq!(
                world
                    .get::<AiCutTreePlot>(cut_worker)
                    .unwrap()
                    .progress_ticks(),
                4
            );
        });
    }
}
