use crate::farming::{FarmInventory, FieldCrop};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::resources::{ResourceAmounts, ResourceInventory, ResourceKind};
use bevy_ecs::prelude::*;

pub const DEFAULT_WAREHOUSE_INVENTORY_MAX_SIZE: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Warehouse,
    TownHall,
    Farm,
    Field,
}

impl BuildingKind {
    pub const ALL: [Self; 4] = [Self::Warehouse, Self::TownHall, Self::Farm, Self::Field];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Warehouse => "Warehouse",
            Self::TownHall => "TownHall",
            Self::Farm => "Farm",
            Self::Field => "Field",
        }
    }

    pub const fn definition(self) -> BuildingDefinition {
        match self {
            Self::Warehouse => BuildingDefinition {
                kind: self,
                width: 2,
                height: 2,
                construction_cost: ResourceAmounts::new(40, 20, 0, 0),
            },
            Self::TownHall => BuildingDefinition {
                kind: self,
                width: 3,
                height: 3,
                construction_cost: ResourceAmounts::new(80, 60, 0, 20),
            },
            Self::Farm => BuildingDefinition {
                kind: self,
                width: 3,
                height: 3,
                construction_cost: ResourceAmounts::new(20, 30, 0, 0),
            },
            Self::Field => BuildingDefinition {
                kind: self,
                width: 1,
                height: 1,
                construction_cost: ResourceAmounts::new(5, 1, 0, 0),
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
    FieldRequiresFarm,
}

pub fn place_building_blueprint(
    world: &mut World,
    kind: BuildingKind,
    origin: CellCoord,
) -> Result<Entity, BuildingPlacementError> {
    let footprint = validate_building_blueprint_placement(world, kind, origin)?;

    let entity = world.spawn(BuildingBlueprintBundle::new(kind, footprint));

    Ok(entity.id())
}

pub fn validate_building_blueprint_placement(
    world: &World,
    kind: BuildingKind,
    origin: CellCoord,
) -> Result<BuildingFootprint, BuildingPlacementError> {
    if kind == BuildingKind::Field {
        return Err(BuildingPlacementError::FieldRequiresFarm);
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

    Ok(footprint)
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
) {
    for (entity, blueprint, progress) in &blueprints {
        let cost = blueprint.kind.definition().construction_cost();
        if !progress.is_complete(cost) {
            continue;
        }

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
    }
}
