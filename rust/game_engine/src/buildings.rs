use crate::collision::{resource_node_at, terrain_allows_building, terrain_at};
use crate::farming::{FarmInventory, FieldCrop};
use crate::forestry::{ForesterLodgeInventory, TreePlotGrowth};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::housing::House;
use crate::navigation::refresh_navigation_snapshot_cells;
use crate::refining::{RefineryInventory, RefineryProduction};
use crate::resources::{ResourceAmounts, ResourceInventory, ResourceKind};
use bevy_ecs::prelude::*;

pub const DEFAULT_WAREHOUSE_INVENTORY_MAX_SIZE: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Warehouse,
    TownHall,
    Sawmill,
    Stoneworks,
    Kitchen,
    Farm,
    Field,
    ForesterLodge,
    TreePlot,
    SmallHouse,
    MediumHouse,
    LargeHouse,
}

impl BuildingKind {
    pub const ALL: [Self; 12] = [
        Self::Warehouse,
        Self::TownHall,
        Self::Sawmill,
        Self::Stoneworks,
        Self::Kitchen,
        Self::Farm,
        Self::Field,
        Self::ForesterLodge,
        Self::TreePlot,
        Self::SmallHouse,
        Self::MediumHouse,
        Self::LargeHouse,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Warehouse => "Warehouse",
            Self::TownHall => "TownHall",
            Self::Sawmill => "Sawmill",
            Self::Stoneworks => "Stoneworks",
            Self::Kitchen => "Kitchen",
            Self::Farm => "Farm",
            Self::Field => "Field",
            Self::ForesterLodge => "Forester's Lodge",
            Self::TreePlot => "Tree Plot",
            Self::SmallHouse => "Small House",
            Self::MediumHouse => "Medium House",
            Self::LargeHouse => "Large House",
        }
    }

    pub const fn definition(self) -> BuildingDefinition {
        match self {
            Self::Warehouse => BuildingDefinition {
                kind: self,
                width: 2,
                height: 2,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 40)
                    .with(ResourceKind::StoneBlocks, 20),
                housing_capacity: None,
            },
            Self::TownHall => BuildingDefinition {
                kind: self,
                width: 3,
                height: 3,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 80)
                    .with(ResourceKind::StoneBlocks, 60)
                    .with(ResourceKind::Gold, 20),
                housing_capacity: None,
            },
            Self::Sawmill => BuildingDefinition {
                kind: self,
                width: 2,
                height: 2,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Wood, 20)
                    .with(ResourceKind::Stone, 10),
                housing_capacity: None,
            },
            Self::Stoneworks => BuildingDefinition {
                kind: self,
                width: 2,
                height: 2,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Wood, 20)
                    .with(ResourceKind::Stone, 20),
                housing_capacity: None,
            },
            Self::Kitchen => BuildingDefinition {
                kind: self,
                width: 2,
                height: 2,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 20)
                    .with(ResourceKind::StoneBlocks, 10),
                housing_capacity: None,
            },
            Self::Farm => BuildingDefinition {
                kind: self,
                width: 3,
                height: 3,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 20)
                    .with(ResourceKind::StoneBlocks, 30),
                housing_capacity: None,
            },
            Self::Field => BuildingDefinition {
                kind: self,
                width: 1,
                height: 1,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 5)
                    .with(ResourceKind::StoneBlocks, 1),
                housing_capacity: None,
            },
            Self::ForesterLodge => BuildingDefinition {
                kind: self,
                width: 3,
                height: 3,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 20)
                    .with(ResourceKind::StoneBlocks, 30),
                housing_capacity: None,
            },
            Self::TreePlot => BuildingDefinition {
                kind: self,
                width: 1,
                height: 1,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 5)
                    .with(ResourceKind::StoneBlocks, 1),
                housing_capacity: None,
            },
            Self::SmallHouse => BuildingDefinition {
                kind: self,
                width: 1,
                height: 1,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 10)
                    .with(ResourceKind::StoneBlocks, 5),
                housing_capacity: Some(2),
            },
            Self::MediumHouse => BuildingDefinition {
                kind: self,
                width: 2,
                height: 2,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 30)
                    .with(ResourceKind::StoneBlocks, 15),
                housing_capacity: Some(4),
            },
            Self::LargeHouse => BuildingDefinition {
                kind: self,
                width: 3,
                height: 3,
                construction_cost: ResourceAmounts::zero()
                    .with(ResourceKind::Planks, 60)
                    .with(ResourceKind::StoneBlocks, 30),
                housing_capacity: Some(8),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingDefinition {
    kind: BuildingKind,
    width: usize,
    height: usize,
    construction_cost: ResourceAmounts,
    housing_capacity: Option<usize>,
}

impl BuildingDefinition {
    pub const fn kind(self) -> BuildingKind {
        self.kind
    }

    pub const fn width(self) -> usize {
        self.width
    }

    pub const fn height(self) -> usize {
        self.height
    }

    pub const fn construction_cost(self) -> ResourceAmounts {
        self.construction_cost
    }

    pub const fn housing_capacity(self) -> Option<usize> {
        self.housing_capacity
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct BuildingBlueprint {
    pub kind: BuildingKind,
    pub footprint: BuildingFootprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Building {
    pub kind: BuildingKind,
    pub footprint: BuildingFootprint,
}

impl Building {
    pub const fn new(kind: BuildingKind, footprint: BuildingFootprint) -> Self {
        Self { kind, footprint }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingFootprint {
    origin: CellCoord,
    width: usize,
    height: usize,
}

impl BuildingFootprint {
    pub const fn new(origin: CellCoord, width: usize, height: usize) -> Self {
        Self {
            origin,
            width,
            height,
        }
    }

    pub const fn origin(self) -> CellCoord {
        self.origin
    }

    pub const fn width(self) -> usize {
        self.width
    }

    pub const fn height(self) -> usize {
        self.height
    }

    pub fn contains(self, coord: CellCoord) -> bool {
        let Some((left, top, right, bottom)) = self.bounds() else {
            return false;
        };

        coord.x() >= left && coord.x() < right && coord.y() >= top && coord.y() < bottom
    }

    pub fn iter_coords(self) -> impl Iterator<Item = CellCoord> {
        let origin = self.origin;
        let width = self.width;
        let height = self.height;

        (0..height).flat_map(move |dy| {
            (0..width).filter_map(move |dx| {
                let dx = i32::try_from(dx).ok()?;
                let dy = i32::try_from(dy).ok()?;
                Some(CellCoord::new(
                    origin.x().checked_add(dx)?,
                    origin.y().checked_add(dy)?,
                ))
            })
        })
    }

    pub fn is_within(self, size: GridSize) -> bool {
        if self.width == 0 || self.height == 0 {
            return false;
        }

        let Some((left, top, right, bottom)) = self.bounds() else {
            return false;
        };
        let Some(grid_width) = size.width_i32() else {
            return false;
        };
        let Some(grid_height) = size.height_i32() else {
            return false;
        };

        left >= 0 && top >= 0 && right <= grid_width && bottom <= grid_height
    }

    pub fn overlaps(self, other: Self) -> bool {
        let Some((left, top, right, bottom)) = self.bounds() else {
            return false;
        };
        let Some((other_left, other_top, other_right, other_bottom)) = other.bounds() else {
            return false;
        };

        left < other_right && right > other_left && top < other_bottom && bottom > other_top
    }

    fn bounds(self) -> Option<(i32, i32, i32, i32)> {
        let width = i32::try_from(self.width).ok()?;
        let height = i32::try_from(self.height).ok()?;
        let right = self.origin.x().checked_add(width)?;
        let bottom = self.origin.y().checked_add(height)?;

        Some((self.origin.x(), self.origin.y(), right, bottom))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct ConstructionProgress {
    deposited: ResourceAmounts,
}

impl ConstructionProgress {
    pub const fn new(deposited: ResourceAmounts) -> Self {
        Self { deposited }
    }

    pub const fn deposited(self) -> ResourceAmounts {
        self.deposited
    }

    pub fn remaining(self, cost: ResourceAmounts, kind: crate::resources::ResourceKind) -> u32 {
        cost.get(kind).saturating_sub(self.deposited.get(kind))
    }

    pub fn deposit(
        &mut self,
        kind: crate::resources::ResourceKind,
        amount: u32,
        cost: ResourceAmounts,
    ) -> u32 {
        let deposited = amount.min(self.remaining(cost, kind));
        if deposited == 0 {
            return 0;
        }

        let current = self.deposited.get(kind);
        self.deposited.set(kind, current.saturating_add(deposited));
        deposited
    }

    pub fn is_complete(self, cost: ResourceAmounts) -> bool {
        crate::resources::ResourceKind::ALL
            .into_iter()
            .all(|kind| self.remaining(cost, kind) == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Bundle)]
pub struct BuildingBlueprintBundle {
    blueprint: BuildingBlueprint,
    construction_progress: ConstructionProgress,
}

impl BuildingBlueprintBundle {
    pub const fn new(kind: BuildingKind, footprint: BuildingFootprint) -> Self {
        Self {
            blueprint: BuildingBlueprint { kind, footprint },
            construction_progress: ConstructionProgress::new(ResourceAmounts::zero()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct WarehouseInventory {
    inventory: ResourceInventory,
}

impl WarehouseInventory {
    pub const fn empty() -> Self {
        Self {
            inventory: ResourceInventory::empty(DEFAULT_WAREHOUSE_INVENTORY_MAX_SIZE),
        }
    }

    pub const fn contents(self) -> ResourceAmounts {
        self.inventory.contents()
    }

    pub const fn max_size(self) -> u32 {
        self.inventory.max_size()
    }

    pub const fn used_size(self) -> u32 {
        self.inventory.used_size()
    }

    pub const fn free_size(self) -> u32 {
        self.inventory.free_size()
    }

    pub fn consume(&mut self, kind: ResourceKind, amount: u32) -> bool {
        self.inventory.consume(kind, amount)
    }

    pub fn add(&mut self, kind: ResourceKind, amount: u32) -> bool {
        self.inventory.add(kind, amount)
    }
}

impl Default for WarehouseInventory {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingPlacementError {
    OutOfBounds,
    OverlapsBuilding,
    InvalidTerrain,
    BlockedByResourceNode,
    FieldRequiresFarm,
    TreePlotRequiresLodge,
}

pub fn place_building_blueprint(
    world: &mut World,
    kind: BuildingKind,
    origin: CellCoord,
) -> Result<Entity, BuildingPlacementError> {
    let footprint = validate_building_blueprint_placement(world, kind, origin)?;

    let entity = world
        .spawn(BuildingBlueprintBundle::new(kind, footprint))
        .id();
    refresh_navigation_snapshot_cells(world, footprint.iter_coords());

    Ok(entity)
}

pub fn validate_building_blueprint_placement(
    world: &World,
    kind: BuildingKind,
    origin: CellCoord,
) -> Result<BuildingFootprint, BuildingPlacementError> {
    match kind {
        BuildingKind::Field => return Err(BuildingPlacementError::FieldRequiresFarm),
        BuildingKind::TreePlot => return Err(BuildingPlacementError::TreePlotRequiresLodge),
        _ => {}
    }

    validate_building_footprint_placement(world, kind, origin)
}

pub(crate) fn validate_building_footprint_placement(
    world: &World,
    kind: BuildingKind,
    origin: CellCoord,
) -> Result<BuildingFootprint, BuildingPlacementError> {
    let definition = kind.definition();
    let footprint = BuildingFootprint::new(origin, definition.width(), definition.height());
    let size = world.resource::<Grid>().size();

    if !footprint.is_within(size) {
        return Err(BuildingPlacementError::OutOfBounds);
    }
    if overlaps_existing_blueprint(world, footprint) {
        return Err(BuildingPlacementError::OverlapsBuilding);
    }
    if has_invalid_terrain(world, kind, footprint) {
        return Err(BuildingPlacementError::InvalidTerrain);
    }
    if overlaps_resource_node(world, footprint) {
        return Err(BuildingPlacementError::BlockedByResourceNode);
    }

    Ok(footprint)
}

fn has_invalid_terrain(world: &World, kind: BuildingKind, footprint: BuildingFootprint) -> bool {
    footprint
        .iter_coords()
        .any(|coord| match terrain_at(world, coord) {
            Some(terrain) => !terrain_allows_building(kind, terrain),
            None => true,
        })
}

fn overlaps_resource_node(world: &World, footprint: BuildingFootprint) -> bool {
    footprint
        .iter_coords()
        .any(|coord| resource_node_at(world, coord))
}

fn overlaps_existing_blueprint(world: &World, footprint: BuildingFootprint) -> bool {
    let overlaps_blueprint = world
        .try_query::<&BuildingBlueprint>()
        .map(|mut query| {
            query
                .iter(world)
                .any(|blueprint| footprint.overlaps(blueprint.footprint))
        })
        .unwrap_or(false);
    if overlaps_blueprint {
        return true;
    }

    world
        .try_query::<&Building>()
        .map(|mut query| {
            query
                .iter(world)
                .any(|building| footprint.overlaps(building.footprint))
        })
        .unwrap_or(false)
}

pub fn system_complete_building_construction(
    mut commands: Commands,
    blueprints: Query<(Entity, &BuildingBlueprint, &ConstructionProgress)>,
    houses: Query<&House>,
) {
    let mut completed = blueprints
        .iter()
        .filter(|(_, blueprint, progress)| {
            progress.is_complete(blueprint.kind.definition().construction_cost())
        })
        .collect::<Vec<_>>();
    completed.sort_by_key(|(entity, _, _)| entity.index());

    let mut next_house_order = houses
        .iter()
        .map(|house| house.completion_order())
        .max()
        .map_or(0, |order| order.saturating_add(1));

    for (entity, blueprint, _) in completed {
        let cost = blueprint.kind.definition().construction_cost();
        debug_assert!(blueprints
            .get(entity)
            .is_ok_and(|(_, _, progress)| progress.is_complete(cost)));

        let mut entity_commands = commands.entity(entity);
        entity_commands
            .remove::<BuildingBlueprint>()
            .remove::<ConstructionProgress>()
            .insert(Building::new(blueprint.kind, blueprint.footprint));
        if blueprint.kind == BuildingKind::Warehouse {
            entity_commands.insert(WarehouseInventory::empty());
        }
        if blueprint.kind == BuildingKind::Farm {
            entity_commands.insert(FarmInventory::empty());
        }
        if blueprint.kind == BuildingKind::Field {
            entity_commands.insert(FieldCrop::seedable());
        }
        if blueprint.kind == BuildingKind::ForesterLodge {
            entity_commands.insert(ForesterLodgeInventory::empty());
        }
        if blueprint.kind == BuildingKind::TreePlot {
            entity_commands.insert(TreePlotGrowth::seedable());
        }
        if matches!(
            blueprint.kind,
            BuildingKind::Sawmill | BuildingKind::Stoneworks | BuildingKind::Kitchen
        ) {
            entity_commands.insert((RefineryInventory::empty(), RefineryProduction::default()));
        }
        if let Some(capacity) = blueprint.kind.definition().housing_capacity() {
            entity_commands.insert(House::new(capacity, next_house_order));
            next_house_order = next_house_order.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Npc, NpcPosition, ResourceNode, TerrainKind};
    use crate::grid::GridSize;
    use crate::resources::ResourceKind;
    use crate::tile::{TileBundle, TileIndex};

    #[test]
    fn footprint_iter_coords_returns_row_major_cells() {
        let footprint = BuildingFootprint::new(CellCoord::new(2, 3), 2, 3);

        assert_eq!(
            footprint.iter_coords().collect::<Vec<_>>(),
            vec![
                CellCoord::new(2, 3),
                CellCoord::new(3, 3),
                CellCoord::new(2, 4),
                CellCoord::new(3, 4),
                CellCoord::new(2, 5),
                CellCoord::new(3, 5),
            ]
        );
    }

    #[test]
    fn major_buildings_accept_grass_dirt_and_sand() {
        for kind in [
            BuildingKind::Warehouse,
            BuildingKind::TownHall,
            BuildingKind::Sawmill,
            BuildingKind::Stoneworks,
            BuildingKind::Kitchen,
            BuildingKind::Farm,
        ] {
            for terrain in [TerrainKind::Grass, TerrainKind::Dirt, TerrainKind::Sand] {
                let world = world_with_default_terrain(terrain);

                assert_eq!(
                    validate_building_blueprint_placement(&world, kind, CellCoord::new(0, 0)),
                    Ok(BuildingFootprint::new(
                        CellCoord::new(0, 0),
                        kind.definition().width(),
                        kind.definition().height(),
                    ))
                );
            }
        }
    }

    #[test]
    fn major_buildings_reject_water() {
        for kind in [
            BuildingKind::Warehouse,
            BuildingKind::TownHall,
            BuildingKind::Sawmill,
            BuildingKind::Stoneworks,
            BuildingKind::Kitchen,
            BuildingKind::Farm,
        ] {
            let world = world_with_default_terrain(TerrainKind::Water);

            assert_eq!(
                validate_building_blueprint_placement(&world, kind, CellCoord::new(0, 0)),
                Err(BuildingPlacementError::InvalidTerrain)
            );
        }
    }

    #[test]
    fn building_placement_rejects_resource_node_overlap() {
        let mut world = world_with_default_terrain(TerrainKind::Grass);
        insert_resource_node(&mut world, CellCoord::new(1, 1));

        assert_eq!(
            validate_building_blueprint_placement(
                &world,
                BuildingKind::Warehouse,
                CellCoord::new(0, 0),
            ),
            Err(BuildingPlacementError::BlockedByResourceNode)
        );
    }

    #[test]
    fn building_placement_allows_npc_overlap() {
        let mut world = world_with_default_terrain(TerrainKind::Grass);
        world.spawn((Npc, NpcPosition::new(CellCoord::new(1, 1))));

        assert_eq!(
            validate_building_blueprint_placement(
                &world,
                BuildingKind::Warehouse,
                CellCoord::new(0, 0),
            ),
            Ok(BuildingFootprint::new(CellCoord::new(0, 0), 2, 2))
        );
    }

    #[test]
    fn building_placement_rejects_existing_blueprint_overlap() {
        let mut world = world_with_default_terrain(TerrainKind::Grass);
        world.spawn(BuildingBlueprintBundle::new(
            BuildingKind::Warehouse,
            BuildingFootprint::new(CellCoord::new(1, 1), 2, 2),
        ));

        assert_eq!(
            validate_building_blueprint_placement(
                &world,
                BuildingKind::Warehouse,
                CellCoord::new(2, 2),
            ),
            Err(BuildingPlacementError::OverlapsBuilding)
        );
    }

    #[test]
    fn building_placement_rejects_constructed_building_overlap() {
        let mut world = world_with_default_terrain(TerrainKind::Grass);
        world.spawn(Building::new(
            BuildingKind::Warehouse,
            BuildingFootprint::new(CellCoord::new(1, 1), 2, 2),
        ));

        assert_eq!(
            validate_building_blueprint_placement(
                &world,
                BuildingKind::Warehouse,
                CellCoord::new(2, 2),
            ),
            Err(BuildingPlacementError::OverlapsBuilding)
        );
    }

    fn world_with_default_terrain(terrain: TerrainKind) -> World {
        let size = GridSize::new(8, 8);
        let mut world = World::new();
        world.insert_resource(Grid::new(size.width(), size.height()));
        let mut index = TileIndex::new(size);
        for coord in size.iter_coords() {
            let entity = world
                .spawn(TileBundle::new_with_terrain(coord, terrain))
                .id();
            assert!(index.set(coord, entity));
        }
        world.insert_resource(index);
        world
    }

    fn insert_resource_node(world: &mut World, coord: CellCoord) {
        let tile = world
            .resource::<TileIndex>()
            .get(coord)
            .expect("test tile should exist in index");
        world.entity_mut(tile).insert(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 10,
        });
    }
}
