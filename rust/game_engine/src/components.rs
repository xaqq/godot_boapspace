use crate::grid::CellCoord;
use crate::resources::{ResourceAmounts, ResourceInventory, ResourceKind};
use crate::time::SIMULATION_TICKS_PER_DAY;
use bevy_ecs::prelude::{Component, Entity};
use std::time::Duration;

pub const SUBTILE_UNITS_PER_TILE: i32 = 1024;
pub const HALF_SUBTILE_UNITS_PER_TILE: i32 = SUBTILE_UNITS_PER_TILE / 2;
pub const DEFAULT_MAX_VELOCITY_UNITS_PER_TICK: u32 = 16;
pub const DEFAULT_NPC_INVENTORY_MAX_SIZE: u32 = 100;
pub const FOOD_POUCH_CAPACITY: u32 = 100;
pub const CARRIED_RESOURCE_CAPACITY: u32 = 5;
pub const NPC_HUNGER_HUNGRY_THRESHOLD: u32 = SIMULATION_TICKS_PER_DAY;
pub const NPC_HUNGER_FULL_SATIATION: u32 = 2 * SIMULATION_TICKS_PER_DAY;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Component)]
pub enum NpcAppearance {
    #[default]
    Colonist,
    Engineer,
    Botanist,
    Miner,
    Scout,
}

impl NpcAppearance {
    pub const ALL: [Self; 5] = [
        Self::Colonist,
        Self::Engineer,
        Self::Botanist,
        Self::Miner,
        Self::Scout,
    ];
}

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
pub struct AiKeepEnoughFoodInInventory {
    start_threshold: u32,
    target: u32,
}

impl AiKeepEnoughFoodInInventory {
    pub const fn new(start_threshold: u32, target: u32) -> Self {
        assert!(start_threshold <= target);
        Self {
            start_threshold,
            target,
        }
    }

    pub const fn start_threshold(self) -> u32 {
        self.start_threshold
    }

    pub const fn target(self) -> u32 {
        self.target
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiSearchForFood;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiConstructBuilding {
    blueprint: Entity,
    target_kind: Option<ResourceKind>,
}

impl AiConstructBuilding {
    pub const fn new(blueprint: Entity) -> Self {
        Self {
            blueprint,
            target_kind: None,
        }
    }

    pub const fn blueprint(self) -> Entity {
        self.blueprint
    }

    pub const fn target_kind(self) -> Option<ResourceKind> {
        self.target_kind
    }

    pub const fn set_target_kind(&mut self, target_kind: ResourceKind) {
        self.target_kind = Some(target_kind);
    }

    pub const fn clear_target_kind(&mut self) {
        self.target_kind = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiIdleRoam {
    origin: CellCoord,
    dwell_ticks_remaining: u32,
    next_offset_index: usize,
}

impl AiIdleRoam {
    pub const fn new(origin: CellCoord, dwell_ticks_remaining: u32) -> Self {
        Self {
            origin,
            dwell_ticks_remaining,
            next_offset_index: 0,
        }
    }

    pub const fn origin(self) -> CellCoord {
        self.origin
    }

    pub const fn dwell_ticks_remaining(self) -> u32 {
        self.dwell_ticks_remaining
    }

    pub const fn next_offset_index(self) -> usize {
        self.next_offset_index
    }

    pub fn advance_dwell(&mut self) {
        self.dwell_ticks_remaining = self.dwell_ticks_remaining.saturating_sub(1);
    }

    pub const fn reset_dwell(&mut self, dwell_ticks: u32) {
        self.dwell_ticks_remaining = dwell_ticks;
    }

    pub const fn set_next_offset_index(&mut self, next_offset_index: usize) {
        self.next_offset_index = next_offset_index;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiGatherResource {
    target: Entity,
    progress_ticks: u32,
}

impl AiGatherResource {
    pub const fn new(target: Entity) -> Self {
        Self {
            target,
            progress_ticks: 0,
        }
    }

    pub const fn target(self) -> Entity {
        self.target
    }

    pub const fn progress_ticks(self) -> u32 {
        self.progress_ticks
    }

    pub fn advance_tick(&mut self) {
        self.progress_ticks = self.progress_ticks.saturating_add(1);
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
    satiation_level: u32,
}

impl NpcHunger {
    pub const MIN_SATIATION_LEVEL: u32 = 0;
    pub const MAX_SATIATION_LEVEL: u32 = NPC_HUNGER_FULL_SATIATION;

    pub const fn fed() -> Self {
        Self {
            satiation_level: Self::MAX_SATIATION_LEVEL,
        }
    }

    pub const fn new(satiation_level: u32) -> Self {
        Self { satiation_level }
    }

    pub const fn satiation_level(self) -> u32 {
        self.satiation_level
    }

    pub fn state(self) -> HungerState {
        if self.satiation_level == 0 {
            HungerState::Starving
        } else if self.satiation_level <= NPC_HUNGER_HUNGRY_THRESHOLD {
            HungerState::Hungry
        } else {
            HungerState::Fed
        }
    }

    pub fn advance_tick(&mut self, food_pouch: &mut FoodPouch) {
        let was_fed = self.state() == HungerState::Fed;

        if self.satiation_level > 0 {
            self.satiation_level -= 1;
        }

        if self.satiation_level <= NPC_HUNGER_HUNGRY_THRESHOLD && food_pouch.consume(1) {
            self.satiation_level = if was_fed {
                NPC_HUNGER_FULL_SATIATION
            } else {
                NPC_HUNGER_FULL_SATIATION.saturating_sub(1)
            };
        }
    }
}

impl Default for NpcHunger {
    fn default() -> Self {
        Self::fed()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct FoodPouch {
    amount: u32,
}

impl FoodPouch {
    pub const fn empty() -> Self {
        Self { amount: 0 }
    }

    pub const fn new(amount: u32) -> Self {
        assert!(amount <= FOOD_POUCH_CAPACITY);
        Self { amount }
    }

    pub const fn amount(self) -> u32 {
        self.amount
    }

    pub const fn capacity(self) -> u32 {
        FOOD_POUCH_CAPACITY
    }

    pub const fn free_size(self) -> u32 {
        FOOD_POUCH_CAPACITY - self.amount
    }

    pub fn add(&mut self, amount: u32) -> bool {
        let Some(new_amount) = self.amount.checked_add(amount) else {
            return false;
        };
        if new_amount > FOOD_POUCH_CAPACITY {
            return false;
        }
        self.amount = new_amount;
        true
    }

    pub fn consume(&mut self, amount: u32) -> bool {
        if self.amount < amount {
            return false;
        }
        self.amount -= amount;
        true
    }
}

impl Default for FoodPouch {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CarriedResourceStack {
    kind: ResourceKind,
    amount: u32,
}

impl CarriedResourceStack {
    pub const fn kind(self) -> ResourceKind {
        self.kind
    }

    pub const fn amount(self) -> u32 {
        self.amount
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Default)]
pub struct CarriedResource {
    stack: Option<CarriedResourceStack>,
}

pub const WHEELBARROW_CAPACITY: u32 = 25;

/// Simulation-owned hauling equipment. An equipped but empty wheelbarrow is
/// represented by `stack == None`; once loaded it can carry exactly one
/// resource kind and replaces the NPC's normal [`CarriedResource`] cargo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Default)]
pub struct Wheelbarrow {
    stack: Option<CarriedResourceStack>,
}

impl Wheelbarrow {
    pub const fn empty() -> Self {
        Self { stack: None }
    }

    pub const fn of(kind: ResourceKind, amount: u32) -> Self {
        assert!(amount <= WHEELBARROW_CAPACITY);
        if amount == 0 {
            Self::empty()
        } else {
            Self {
                stack: Some(CarriedResourceStack { kind, amount }),
            }
        }
    }

    pub const fn stack(self) -> Option<CarriedResourceStack> {
        self.stack
    }

    pub const fn contents(self) -> ResourceAmounts {
        match self.stack {
            Some(stack) => ResourceAmounts::of(stack.kind, stack.amount),
            None => ResourceAmounts::zero(),
        }
    }

    pub const fn used_size(self) -> u32 {
        match self.stack {
            Some(stack) => stack.amount,
            None => 0,
        }
    }

    pub const fn free_size(self) -> u32 {
        WHEELBARROW_CAPACITY - self.used_size()
    }

    pub fn add(&mut self, kind: ResourceKind, amount: u32) -> bool {
        if amount == 0 {
            return true;
        }
        match self.stack {
            Some(mut stack) => {
                if stack.kind != kind {
                    return false;
                }
                let Some(new_amount) = stack.amount.checked_add(amount) else {
                    return false;
                };
                if new_amount > WHEELBARROW_CAPACITY {
                    return false;
                }
                stack.amount = new_amount;
                self.stack = Some(stack);
            }
            None => {
                if amount > WHEELBARROW_CAPACITY {
                    return false;
                }
                self.stack = Some(CarriedResourceStack { kind, amount });
            }
        }
        true
    }

    pub fn consume(&mut self, kind: ResourceKind, amount: u32) -> bool {
        if amount == 0 {
            return true;
        }
        let Some(mut stack) = self.stack else {
            return false;
        };
        if stack.kind != kind || stack.amount < amount {
            return false;
        }
        stack.amount -= amount;
        self.stack = (stack.amount > 0).then_some(stack);
        true
    }
}

impl CarriedResource {
    pub const fn empty() -> Self {
        Self { stack: None }
    }

    pub const fn of(kind: ResourceKind, amount: u32) -> Self {
        assert!(amount <= CARRIED_RESOURCE_CAPACITY);
        if amount == 0 {
            Self::empty()
        } else {
            Self {
                stack: Some(CarriedResourceStack { kind, amount }),
            }
        }
    }

    pub const fn stack(self) -> Option<CarriedResourceStack> {
        self.stack
    }

    pub const fn contents(self) -> ResourceAmounts {
        match self.stack {
            Some(stack) => ResourceAmounts::of(stack.kind, stack.amount),
            None => ResourceAmounts::zero(),
        }
    }

    pub const fn used_size(self) -> u32 {
        match self.stack {
            Some(stack) => stack.amount,
            None => 0,
        }
    }

    pub const fn max_size(self) -> u32 {
        CARRIED_RESOURCE_CAPACITY
    }

    pub const fn free_size(self) -> u32 {
        CARRIED_RESOURCE_CAPACITY - self.used_size()
    }

    pub fn add(&mut self, kind: ResourceKind, amount: u32) -> bool {
        if amount == 0 {
            return true;
        }
        match self.stack {
            Some(mut stack) => {
                if stack.kind != kind {
                    return false;
                }
                let Some(new_amount) = stack.amount.checked_add(amount) else {
                    return false;
                };
                if new_amount > CARRIED_RESOURCE_CAPACITY {
                    return false;
                }
                stack.amount = new_amount;
                self.stack = Some(stack);
            }
            None => {
                if amount > CARRIED_RESOURCE_CAPACITY {
                    return false;
                }
                self.stack = Some(CarriedResourceStack { kind, amount });
            }
        }
        true
    }

    pub fn consume(&mut self, kind: ResourceKind, amount: u32) -> bool {
        if amount == 0 {
            return true;
        }
        let Some(mut stack) = self.stack else {
            return false;
        };
        if stack.kind != kind || stack.amount < amount {
            return false;
        }
        stack.amount -= amount;
        self.stack = (stack.amount > 0).then_some(stack);
        true
    }
}

/// Legacy inventory retained for unscheduled compatibility AI helpers.
/// Live NPCs use [`FoodPouch`] and [`CarriedResource`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct NpcInventory {
    inventory: ResourceInventory,
}

impl NpcInventory {
    pub const fn empty() -> Self {
        Self {
            inventory: ResourceInventory::empty(DEFAULT_NPC_INVENTORY_MAX_SIZE),
        }
    }

    pub const fn new(contents: ResourceAmounts) -> Self {
        Self {
            inventory: ResourceInventory::new(contents, DEFAULT_NPC_INVENTORY_MAX_SIZE),
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

impl Default for NpcInventory {
    fn default() -> Self {
        Self::empty()
    }
}
