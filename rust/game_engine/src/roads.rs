use std::collections::{HashMap, HashSet};

use bevy_ecs::prelude::*;

use crate::buildings::{
    Building, BuildingBlueprint, ConstructionProgress, CONSTRUCTION_LABOR_TICKS_PER_CELL,
};
use crate::collision::{resource_node_at, terrain_at};
use crate::components::TerrainKind;
use crate::grid::CellCoord;
use crate::navigation::refresh_navigation_snapshot_cells;
use crate::resources::{ResourceAmounts, ResourceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoadTier {
    DirtPath,
    Cobblestone,
    Flagstone,
}

impl RoadTier {
    pub const ALL: [Self; 3] = [Self::DirtPath, Self::Cobblestone, Self::Flagstone];

    pub const fn label(self) -> &'static str {
        match self {
            Self::DirtPath => "Dirt Path",
            Self::Cobblestone => "Cobblestone Road",
            Self::Flagstone => "Flagstone Road",
        }
    }

    pub const fn material_cost(self) -> ResourceAmounts {
        match self {
            Self::DirtPath => ResourceAmounts::zero(),
            Self::Cobblestone => ResourceAmounts::zero().with(ResourceKind::Stone, 1),
            Self::Flagstone => ResourceAmounts::zero().with(ResourceKind::StoneBlocks, 1),
        }
    }

    pub const fn movement_ratio(self) -> (u32, u32) {
        match self {
            Self::DirtPath => (3, 2),
            Self::Cobblestone => (2, 1),
            Self::Flagstone => (3, 1),
        }
    }

    pub const fn traversal_weight(self) -> u32 {
        match self {
            Self::DirtPath => 4,
            Self::Cobblestone => 3,
            Self::Flagstone => 2,
        }
    }
}

pub const NORMAL_TRAVERSAL_WEIGHT: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Road {
    pub coord: CellCoord,
    pub tier: RoadTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct RoadBlueprint {
    pub coord: CellCoord,
    pub target_tier: RoadTier,
}

#[derive(Debug, Default, Resource)]
pub struct RoadMap {
    entities: HashMap<CellCoord, Entity>,
}

impl RoadMap {
    pub fn entity_at(&self, coord: CellCoord) -> Option<Entity> {
        self.entities.get(&coord).copied()
    }

    pub(crate) fn insert(&mut self, coord: CellCoord, entity: Entity) {
        self.entities.insert(coord, entity);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoadPlacementError {
    OutOfBoundsOrMissingTile,
    InvalidTerrain,
    BlockedByResourceNode,
    OverlapsBuildingOrPlot,
    PendingRoadOperation,
    SameOrHigherTier,
    NotWalkableGround,
}

impl RoadPlacementError {
    pub const fn label(self) -> &'static str {
        match self {
            Self::OutOfBoundsOrMissingTile => "outside the surface or missing a tile",
            Self::InvalidTerrain => "roads require Grass, Dirt, or Sand",
            Self::BlockedByResourceNode => "blocked by a resource node",
            Self::OverlapsBuildingOrPlot => "overlaps a building, plot, or blueprint",
            Self::PendingRoadOperation => "a road blueprint or upgrade is already pending",
            Self::SameOrHigherTier => "the completed road is the same or a higher tier",
            Self::NotWalkableGround => "the cell does not support ground movement",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadPlacementCellResult {
    pub coord: CellCoord,
    pub errors: Vec<RoadPlacementError>,
}

impl RoadPlacementCellResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadPlacementBatchResult {
    pub cells: Vec<RoadPlacementCellResult>,
    pub aggregate_cost: ResourceAmounts,
}

impl RoadPlacementBatchResult {
    pub fn is_valid(&self) -> bool {
        !self.cells.is_empty() && self.cells.iter().all(RoadPlacementCellResult::is_valid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoadCellView {
    pub coord: CellCoord,
    pub completed_tier: Option<RoadTier>,
    pub target_tier: Option<RoadTier>,
    pub construction: Option<ConstructionProgress>,
}

pub fn validate_road_placement_batch(
    world: &World,
    tier: RoadTier,
    coords: impl IntoIterator<Item = CellCoord>,
) -> RoadPlacementBatchResult {
    let mut seen = HashSet::new();
    let coords = coords
        .into_iter()
        .filter(|coord| seen.insert(*coord))
        .collect::<Vec<_>>();

    let cells = coords
        .iter()
        .copied()
        .map(|coord| RoadPlacementCellResult {
            coord,
            errors: road_placement_errors(world, tier, coord),
        })
        .collect::<Vec<_>>();

    let mut aggregate_cost = ResourceAmounts::zero();
    for kind in ResourceKind::ALL {
        aggregate_cost.set(
            kind,
            tier.material_cost()
                .get(kind)
                .saturating_mul(u32::try_from(coords.len()).unwrap_or(u32::MAX)),
        );
    }

    RoadPlacementBatchResult {
        cells,
        aggregate_cost,
    }
}

pub fn place_road_blueprints(
    world: &mut World,
    tier: RoadTier,
    coords: impl IntoIterator<Item = CellCoord>,
) -> Result<Vec<Entity>, RoadPlacementBatchResult> {
    let validation = validate_road_placement_batch(world, tier, coords);
    if !validation.is_valid() {
        return Err(validation);
    }

    let mut entities = Vec::with_capacity(validation.cells.len());
    for cell in &validation.cells {
        let existing = road_entity_at(world, cell.coord);
        let progress = ConstructionProgress::new(ResourceAmounts::zero())
            .with_required_labor(CONSTRUCTION_LABOR_TICKS_PER_CELL);
        let entity = if let Some(entity) = existing {
            world.entity_mut(entity).insert((
                RoadBlueprint {
                    coord: cell.coord,
                    target_tier: tier,
                },
                progress,
            ));
            entity
        } else {
            world
                .spawn((
                    RoadBlueprint {
                        coord: cell.coord,
                        target_tier: tier,
                    },
                    progress,
                ))
                .id()
        };
        world.resource_mut::<RoadMap>().insert(cell.coord, entity);
        entities.push(entity);
    }
    Ok(entities)
}

pub fn road_cell_view(world: &World, coord: CellCoord) -> Option<RoadCellView> {
    let entity = road_entity_at(world, coord)?;
    let completed_tier = world.get::<Road>(entity).map(|road| road.tier);
    let target_tier = world
        .get::<RoadBlueprint>(entity)
        .map(|blueprint| blueprint.target_tier);
    if completed_tier.is_none() && target_tier.is_none() {
        return None;
    }
    Some(RoadCellView {
        coord,
        completed_tier,
        target_tier,
        construction: world.get::<ConstructionProgress>(entity).copied(),
    })
}

pub fn completed_road_tier_at(world: &World, coord: CellCoord) -> Option<RoadTier> {
    let entity = road_entity_at(world, coord)?;
    world.get::<Road>(entity).map(|road| road.tier)
}

pub fn road_entity_at(world: &World, coord: CellCoord) -> Option<Entity> {
    if let Some(entity) = world
        .get_resource::<RoadMap>()
        .and_then(|index| index.entity_at(coord))
    {
        return Some(entity);
    }
    world
        .try_query::<(Entity, Option<&Road>, Option<&RoadBlueprint>)>()
        .and_then(|mut query| {
            query.iter(world).find_map(|(entity, road, blueprint)| {
                let entity_coord = road
                    .map(|road| road.coord)
                    .or_else(|| blueprint.map(|b| b.coord));
                (entity_coord == Some(coord)).then_some(entity)
            })
        })
}

pub fn complete_road_construction(world: &mut World) {
    let mut query = world.query::<(Entity, &RoadBlueprint, &ConstructionProgress)>();
    let mut completed = query
        .iter(world)
        .filter_map(|(entity, blueprint, progress)| {
            progress
                .is_complete(blueprint.target_tier.material_cost())
                .then_some((entity, *blueprint))
        })
        .collect::<Vec<_>>();
    completed.sort_unstable_by_key(|(entity, _)| entity.to_bits());

    for (entity, blueprint) in completed {
        world
            .entity_mut(entity)
            .remove::<RoadBlueprint>()
            .remove::<ConstructionProgress>()
            .insert(Road {
                coord: blueprint.coord,
                tier: blueprint.target_tier,
            });
        refresh_navigation_snapshot_cells(world, [blueprint.coord]);
    }
}

fn road_placement_errors(
    world: &World,
    tier: RoadTier,
    coord: CellCoord,
) -> Vec<RoadPlacementError> {
    let mut errors = Vec::new();
    let terrain = terrain_at(world, coord);
    if terrain.is_none() {
        errors.push(RoadPlacementError::OutOfBoundsOrMissingTile);
        return errors;
    }
    if !matches!(
        terrain,
        Some(TerrainKind::Grass | TerrainKind::Dirt | TerrainKind::Sand)
    ) {
        errors.push(RoadPlacementError::InvalidTerrain);
    }
    if resource_node_at(world, coord) {
        errors.push(RoadPlacementError::BlockedByResourceNode);
    }
    if building_or_blueprint_at(world, coord) {
        errors.push(RoadPlacementError::OverlapsBuildingOrPlot);
    }
    if let Some(entity) = road_entity_at(world, coord) {
        if world.get::<RoadBlueprint>(entity).is_some() {
            errors.push(RoadPlacementError::PendingRoadOperation);
        }
        if world
            .get::<Road>(entity)
            .is_some_and(|road| road.tier >= tier)
        {
            errors.push(RoadPlacementError::SameOrHigherTier);
        }
    }
    if matches!(terrain, Some(TerrainKind::Water)) {
        errors.push(RoadPlacementError::NotWalkableGround);
    }
    errors.sort_unstable();
    errors.dedup();
    errors
}

fn building_or_blueprint_at(world: &World, coord: CellCoord) -> bool {
    world
        .try_query::<&BuildingBlueprint>()
        .is_some_and(|mut query| {
            query
                .iter(world)
                .any(|blueprint| blueprint.footprint.contains(coord))
        })
        || world.try_query::<&Building>().is_some_and(|mut query| {
            query
                .iter(world)
                .any(|building| building.footprint.contains(coord))
        })
}
