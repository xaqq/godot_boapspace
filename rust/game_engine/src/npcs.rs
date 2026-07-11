pub use crate::components::{
    AiIdleRoam, AiKeepEnoughFoodInInventory, BirthDate, HungerState, MaxVelocity, MovementFacing,
    MovementTarget, Npc, NpcAppearance, NpcHunger, NpcInventory, NpcName, NpcPosition,
    SubtileOffset, Velocity,
};
pub use crate::skills::{
    skill_percent, Cook, NpcSkills, Sawyer, SkillKind, SkillRank, Stonemason, MAX_SKILL_VALUE,
};

use crate::ai::{DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD, DEFAULT_NPC_FOOD_INVENTORY_TARGET};
use crate::farming::Farmer;
use crate::forestry::Forester;
use crate::grid::{CellCoord, Grid};
use crate::resources::{ResourceAmounts, ResourceKind};
use crate::time::{DAYS_PER_YEAR, SECONDS_PER_DAY};
use bevy_ecs::prelude::*;
use std::time::Duration;

pub const INITIAL_NPC_NAME: &str = "Mara Voss";
pub const INITIAL_NPC_BIRTH_DAY: u64 = 320;
pub const DEFAULT_WORLD_DATE_TIME_DAY: u64 = 0;
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_MINUTE: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitialNpcSpec {
    pub name: &'static str,
    pub birth_day: u64,
    pub appearance: NpcAppearance,
    offset_from_center: (i32, i32),
}

impl InitialNpcSpec {
    const fn new(
        name: &'static str,
        birth_day: u64,
        appearance: NpcAppearance,
        offset_from_center: (i32, i32),
    ) -> Self {
        Self {
            name,
            birth_day,
            appearance,
            offset_from_center,
        }
    }
}

pub const INITIAL_NPC_SPECS: [InitialNpcSpec; 5] = [
    InitialNpcSpec::new(
        INITIAL_NPC_NAME,
        INITIAL_NPC_BIRTH_DAY,
        NpcAppearance::Colonist,
        (0, 0),
    ),
    InitialNpcSpec::new("Ilya Ren", 326, NpcAppearance::Engineer, (1, 0)),
    InitialNpcSpec::new("Sera Nox", 334, NpcAppearance::Botanist, (0, 1)),
    InitialNpcSpec::new("Toma Kade", 311, NpcAppearance::Miner, (-1, 0)),
    InitialNpcSpec::new("Vale Arin", 303, NpcAppearance::Scout, (0, -1)),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct WorldDateTime {
    elapsed_since_world_epoch: Duration,
}

impl WorldDateTime {
    pub const fn new(elapsed_since_world_epoch: Duration) -> Self {
        Self {
            elapsed_since_world_epoch,
        }
    }

    pub const fn from_day(day: u64) -> Self {
        Self::new(Duration::from_secs(day * SECONDS_PER_DAY))
    }

    pub const fn elapsed_since_world_epoch(self) -> Duration {
        self.elapsed_since_world_epoch
    }

    pub const fn day(self) -> u64 {
        self.elapsed_since_world_epoch.as_secs() / SECONDS_PER_DAY
    }

    pub fn advance_by(&mut self, delta: Duration) {
        self.elapsed_since_world_epoch = self
            .elapsed_since_world_epoch
            .checked_add(delta)
            .unwrap_or(Duration::MAX);
    }

    pub const fn hour(self) -> u8 {
        ((self.elapsed_since_world_epoch.as_secs() % SECONDS_PER_DAY) / SECONDS_PER_HOUR) as u8
    }

    pub const fn minute(self) -> u8 {
        ((self.elapsed_since_world_epoch.as_secs() % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE) as u8
    }

    pub fn age_years_since(self, birth_date: BirthDate) -> u32 {
        let lived = self
            .elapsed_since_world_epoch
            .checked_sub(birth_date.elapsed_since_world_epoch())
            .unwrap_or_default();
        u32::try_from(lived.as_secs() / (u64::from(DAYS_PER_YEAR) * SECONDS_PER_DAY))
            .unwrap_or(u32::MAX)
    }
}

pub const fn world_duration_from_day(day: u64) -> Duration {
    Duration::from_secs(day * SECONDS_PER_DAY)
}

#[derive(Debug, Clone, PartialEq, Eq, Bundle)]
pub struct InitialNpcBundle {
    npc: Npc,
    appearance: NpcAppearance,
    name: NpcName,
    birth_date: BirthDate,
    position: NpcPosition,
    velocity: Velocity,
    max_velocity: MaxVelocity,
    movement_facing: MovementFacing,
    hunger: NpcHunger,
    inventory: NpcInventory,
    skills: NpcSkills,
    farmer: Farmer,
    forester: Forester,
    sawyer: Sawyer,
    stonemason: Stonemason,
    cook: Cook,
    keep_food_in_inventory: AiKeepEnoughFoodInInventory,
}

impl InitialNpcBundle {
    pub fn new(coord: CellCoord) -> Self {
        Self::from_spec(INITIAL_NPC_SPECS[0], coord)
    }

    pub fn from_spec(spec: InitialNpcSpec, coord: CellCoord) -> Self {
        Self {
            npc: Npc,
            appearance: spec.appearance,
            name: NpcName::new(spec.name),
            birth_date: BirthDate::new(world_duration_from_day(spec.birth_day)),
            position: NpcPosition::new(coord),
            velocity: Velocity::ZERO,
            max_velocity: MaxVelocity::default(),
            movement_facing: MovementFacing::default(),
            hunger: NpcHunger::fed(),
            inventory: NpcInventory::new(ResourceAmounts::of(ResourceKind::Food, 20)),
            skills: NpcSkills::default(),
            farmer: Farmer,
            forester: Forester,
            sawyer: Sawyer,
            stonemason: Stonemason,
            cook: Cook,
            keep_food_in_inventory: AiKeepEnoughFoodInInventory::new(
                DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD,
                DEFAULT_NPC_FOOD_INVENTORY_TARGET,
            ),
        }
    }
}

pub fn spawn_initial_default_npcs(mut commands: Commands, grid: Res<Grid>) {
    let Some(coord) = center_coord(&grid) else {
        return;
    };

    for spec in INITIAL_NPC_SPECS {
        if let Some(coord) = initial_npc_coord(coord, spec, &grid) {
            commands.spawn(InitialNpcBundle::from_spec(spec, coord));
        }
    }
}

pub fn update_npc_hunger(mut npcs: Query<(&mut NpcHunger, &mut NpcInventory), With<Npc>>) {
    for (mut hunger, mut inventory) in &mut npcs {
        hunger.advance_tick(&mut inventory);
    }
}

pub fn center_coord(grid: &Grid) -> Option<CellCoord> {
    let size = grid.size();
    if size.width() == 0 || size.height() == 0 {
        return None;
    }

    let coord = CellCoord::from_usize(size.width() / 2, size.height() / 2)?;
    size.contains(coord).then_some(coord)
}

fn initial_npc_coord(center: CellCoord, spec: InitialNpcSpec, grid: &Grid) -> Option<CellCoord> {
    let coord = CellCoord::new(
        center.x() + spec.offset_from_center.0,
        center.y() + spec.offset_from_center.1,
    );
    grid.size().contains(coord).then_some(coord)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_date_time_age_years_since_birth_date() {
        let world_date_time = WorldDateTime::from_day(12_000);

        assert_eq!(
            world_date_time.age_years_since(BirthDate::new(world_duration_from_day(
                INITIAL_NPC_BIRTH_DAY
            ))),
            32
        );
    }

    #[test]
    fn test_world_date_time_does_not_return_negative_age() {
        let world_date_time = WorldDateTime::from_day(10);

        assert_eq!(
            world_date_time.age_years_since(BirthDate::new(world_duration_from_day(20))),
            0
        );
    }

    #[test]
    fn test_world_date_time_exposes_day_from_duration() {
        let world_date_time = WorldDateTime::new(Duration::from_secs(42 * SECONDS_PER_DAY));

        assert_eq!(world_date_time.day(), 42);
    }

    #[test]
    fn test_world_date_time_advances_by_duration() {
        let mut world_date_time = WorldDateTime::new(Duration::from_secs(42));

        world_date_time.advance_by(Duration::from_secs(12));

        assert_eq!(
            world_date_time.elapsed_since_world_epoch(),
            Duration::from_secs(54)
        );
    }

    #[test]
    fn test_world_date_time_exposes_hour_and_minute() {
        let world_date_time = WorldDateTime::new(Duration::from_secs(
            42 * SECONDS_PER_DAY + 9 * SECONDS_PER_HOUR + 5 * SECONDS_PER_MINUTE + 59,
        ));

        assert_eq!(world_date_time.hour(), 9);
        assert_eq!(world_date_time.minute(), 5);
    }

    #[test]
    fn initial_npc_bundle_includes_zeroed_skills() {
        let mut world = World::new();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(0, 0)))
            .id();
        let skills = world
            .get::<NpcSkills>(npc)
            .expect("initial NPC bundle should include skills");

        for kind in SkillKind::ALL {
            assert_eq!(skills.value(kind), 0);
        }
    }

    #[test]
    fn initial_npc_bundle_includes_default_appearance() {
        let mut world = World::new();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(0, 0)))
            .id();

        assert_eq!(
            world.get::<NpcAppearance>(npc).copied(),
            Some(NpcAppearance::Colonist)
        );
    }

    #[test]
    fn initial_npc_bundle_includes_every_refining_eligibility_tag() {
        let mut world = World::new();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(0, 0)))
            .id();

        assert!(world.get::<Sawyer>(npc).is_some());
        assert!(world.get::<Stonemason>(npc).is_some());
        assert!(world.get::<Cook>(npc).is_some());
    }
}
