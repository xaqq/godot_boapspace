use crate::grid::CellCoord;
use crate::resources::{ResourceAmounts, ResourceKind};
use bevy_ecs::prelude::Component;
use std::time::Duration;

pub const SUBTILE_UNITS_PER_TILE: i32 = 1024;
pub const HALF_SUBTILE_UNITS_PER_TILE: i32 = SUBTILE_UNITS_PER_TILE / 2;
pub const DEFAULT_MAX_VELOCITY_UNITS_PER_TICK: u32 = 16;
pub const NPC_HUNGER_FULL_SATIATION: Duration = Duration::from_secs(86_400);
pub const NPC_HUNGER_STARVING_THRESHOLD: Duration = Duration::from_secs(86_400);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct TilePosition {
    pub coord: CellCoord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Tile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Terrain {
    pub kind: TerrainKind,
}

impl Terrain {
    pub const fn new(kind: TerrainKind) -> Self {
        Self { kind }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerrainKind {
    #[default]
    Grass = 0,
    Sand = 1,
    Dirt = 2,
    Water = 3,
}

impl TerrainKind {
    pub const ALL: [Self; 4] = [Self::Grass, Self::Sand, Self::Dirt, Self::Water];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Grass => "Grass",
            Self::Sand => "Sand",
            Self::Dirt => "Dirt",
            Self::Water => "Water",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct ResourceNode {
    pub kind: ResourceKind,
    pub quantity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Npc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HungerState {
    Fed,
    Hungry,
    Starving,
}

impl HungerState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fed => "Fed",
            Self::Hungry => "Hungry",
            Self::Starving => "Starving",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct NpcName {
    value: String,
}

impl NpcName {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        self.value.as_str()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct BirthDate {
    elapsed_since_world_epoch: Duration,
}

impl BirthDate {
    pub const fn new(elapsed_since_world_epoch: Duration) -> Self {
        Self {
            elapsed_since_world_epoch,
        }
    }

    pub const fn elapsed_since_world_epoch(self) -> Duration {
        self.elapsed_since_world_epoch
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct NpcPosition {
    pub coord: CellCoord,
    pub subtile_offset: SubtileOffset,
}

impl NpcPosition {
    pub const fn new(coord: CellCoord) -> Self {
        Self {
            coord,
            subtile_offset: SubtileOffset::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubtileOffset {
    pub x_units: i32,
    pub y_units: i32,
}

impl SubtileOffset {
    pub const ZERO: Self = Self {
        x_units: 0,
        y_units: 0,
    };

    pub const fn new(x_units: i32, y_units: i32) -> Self {
        Self { x_units, y_units }
    }
}

impl Default for SubtileOffset {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Velocity {
    pub x_units_per_tick: i32,
    pub y_units_per_tick: i32,
}

impl Velocity {
    pub const ZERO: Self = Self {
        x_units_per_tick: 0,
        y_units_per_tick: 0,
    };

    pub const fn new(x_units_per_tick: i32, y_units_per_tick: i32) -> Self {
        Self {
            x_units_per_tick,
            y_units_per_tick,
        }
    }

    pub const fn is_zero(self) -> bool {
        self.x_units_per_tick == 0 && self.y_units_per_tick == 0
    }
}

impl Default for Velocity {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct MaxVelocity {
    pub units_per_tick: u32,
}

impl MaxVelocity {
    pub const fn new(units_per_tick: u32) -> Self {
        Self { units_per_tick }
    }
}

impl Default for MaxVelocity {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_VELOCITY_UNITS_PER_TICK)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct MovementTarget {
    pub coord: CellCoord,
}

impl MovementTarget {
    pub const fn new(coord: CellCoord) -> Self {
        Self { coord }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub enum MovementFacing {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl MovementFacing {
    pub fn from_velocity(velocity: Velocity) -> Option<Self> {
        if velocity.is_zero() {
            return None;
        }

        let angle = (velocity.y_units_per_tick as f64).atan2(velocity.x_units_per_tick as f64);
        let octant = (angle / std::f64::consts::FRAC_PI_4).round() as i32;
        match octant.rem_euclid(8) {
            0 => Some(Self::East),
            1 => Some(Self::SouthEast),
            2 => Some(Self::South),
            3 => Some(Self::SouthWest),
            4 => Some(Self::West),
            5 => Some(Self::NorthWest),
            6 => Some(Self::North),
            7 => Some(Self::NorthEast),
            _ => unreachable!("rem_euclid(8) should only produce 0..=7"),
        }
    }
}

impl Default for MovementFacing {
    fn default() -> Self {
        Self::South
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct NpcHunger {
    satiation_remaining: Duration,
    hunger_duration: Duration,
}

impl NpcHunger {
    pub const fn fed() -> Self {
        Self {
            satiation_remaining: NPC_HUNGER_FULL_SATIATION,
            hunger_duration: Duration::ZERO,
        }
    }

    pub const fn new(satiation_remaining: Duration, hunger_duration: Duration) -> Self {
        Self {
            satiation_remaining,
            hunger_duration,
        }
    }

    pub const fn satiation_remaining(self) -> Duration {
        self.satiation_remaining
    }

    pub const fn hunger_duration(self) -> Duration {
        self.hunger_duration
    }

    pub fn state(self) -> HungerState {
        if !self.satiation_remaining.is_zero() {
            HungerState::Fed
        } else if self.hunger_duration >= NPC_HUNGER_STARVING_THRESHOLD {
            HungerState::Starving
        } else {
            HungerState::Hungry
        }
    }

    pub fn advance_by(&mut self, delta: Duration, inventory: &mut NpcInventory) {
        if delta.is_zero() {
            return;
        }

        let mut unfed_delta = delta;
        if !self.satiation_remaining.is_zero() {
            if self.satiation_remaining > delta {
                self.satiation_remaining -= delta;
                return;
            }

            unfed_delta = delta.saturating_sub(self.satiation_remaining);
            self.satiation_remaining = Duration::ZERO;
        }

        if inventory.consume(ResourceKind::Food, 1) {
            self.satiation_remaining = NPC_HUNGER_FULL_SATIATION.saturating_sub(unfed_delta);
            self.hunger_duration = Duration::ZERO;
            return;
        }

        self.hunger_duration = self
            .hunger_duration
            .checked_add(unfed_delta)
            .unwrap_or(Duration::MAX);
    }
}

impl Default for NpcHunger {
    fn default() -> Self {
        Self::fed()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct NpcInventory {
    contents: ResourceAmounts,
}

impl NpcInventory {
    pub const fn empty() -> Self {
        Self {
            contents: ResourceAmounts::zero(),
        }
    }

    pub const fn new(contents: ResourceAmounts) -> Self {
        Self { contents }
    }

    pub const fn contents(self) -> ResourceAmounts {
        self.contents
    }

    pub fn consume(&mut self, kind: ResourceKind, amount: u32) -> bool {
        let available = self.contents.get(kind);
        if available < amount {
            return false;
        }

        self.contents.set(kind, available - amount);
        true
    }
}

impl Default for NpcInventory {
    fn default() -> Self {
        Self::empty()
    }
}
