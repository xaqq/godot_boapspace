use crate::grid::{CellCoord, Grid, GridSize};
use crate::resources::ResourceAmounts;
use bevy_ecs::prelude::*;

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
pub struct Building {
    pub kind: BuildingKind,
    pub footprint: BuildingFootprint,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct WarehouseInventory {
    contents: ResourceAmounts,
}

impl WarehouseInventory {
    pub const fn empty() -> Self {
        Self {
            contents: ResourceAmounts::zero(),
        }
    }

    pub const fn contents(self) -> ResourceAmounts {
        self.contents
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

    let entity = world.spawn((
        BuildingBlueprint { kind, footprint },
        ConstructionProgress::new(ResourceAmounts::zero()),
    ));

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
    if overlaps_existing_building(world, footprint) {
        return Err(BuildingPlacementError::OverlapsBuilding);
    }

    Ok(footprint)
}

fn overlaps_existing_building(world: &World, footprint: BuildingFootprint) -> bool {
    let overlaps_building = world
        .try_query::<&Building>()
        .map(|mut query| {
            query
                .iter(world)
                .any(|building| footprint.overlaps(building.footprint))
        })
        .unwrap_or(false);
    let overlaps_blueprint = world
        .try_query::<&BuildingBlueprint>()
        .map(|mut query| {
            query
                .iter(world)
                .any(|blueprint| footprint.overlaps(blueprint.footprint))
        })
        .unwrap_or(false);

    overlaps_building || overlaps_blueprint
}
