use crate::grid::{CellCoord, Grid, GridSize};
use crate::resources::{ResourceAmounts, ResourceInventory, ResourceKind};
use bevy_ecs::prelude::*;

pub const DEFAULT_WAREHOUSE_INVENTORY_MAX_SIZE: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Warehouse,
    TownHall,
}

impl BuildingKind {
    pub const ALL: [Self; 2] = [Self::Warehouse, Self::TownHall];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Warehouse => "Warehouse",
            Self::TownHall => "TownHall",
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
    world
        .try_query::<&BuildingBlueprint>()
        .map(|mut query| {
            query
                .iter(world)
                .any(|blueprint| footprint.overlaps(blueprint.footprint))
        })
        .unwrap_or(false)
}
