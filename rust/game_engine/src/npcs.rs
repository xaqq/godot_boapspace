pub use crate::components::{
    BirthDate, HungerState, MaxVelocity, MovementFacing, MovementTarget, Npc, NpcHunger,
    NpcInventory, NpcName, NpcPosition, SubtileOffset, Velocity,
};

use crate::grid::{CellCoord, Grid};
use bevy_ecs::prelude::*;
use std::time::Duration;

pub const INITIAL_NPC_NAME: &str = "Mara Voss";
pub const INITIAL_NPC_BIRTH_DAY: u64 = 320;
pub const DEFAULT_WORLD_DATE_TIME_DAY: u64 = 0;
pub const SECONDS_PER_DAY: u64 = 86_400;
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_MINUTE: u64 = 60;
const DAYS_PER_YEAR: u64 = 365;

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
        u32::try_from(lived.as_secs() / (DAYS_PER_YEAR * SECONDS_PER_DAY)).unwrap_or(u32::MAX)
    }
}

pub const fn world_duration_from_day(day: u64) -> Duration {
    Duration::from_secs(day * SECONDS_PER_DAY)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct SimulationTickDuration {
    duration: Duration,
}

impl SimulationTickDuration {
    pub const fn new(duration: Duration) -> Self {
        Self { duration }
    }

    pub const fn duration(self) -> Duration {
        self.duration
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Bundle)]
pub struct InitialNpcBundle {
    npc: Npc,
    name: NpcName,
    birth_date: BirthDate,
    position: NpcPosition,
    velocity: Velocity,
    max_velocity: MaxVelocity,
    movement_facing: MovementFacing,
    hunger: NpcHunger,
    inventory: NpcInventory,
}

impl InitialNpcBundle {
    pub fn new(coord: CellCoord) -> Self {
        Self {
            npc: Npc,
            name: NpcName::new(INITIAL_NPC_NAME),
            birth_date: BirthDate::new(world_duration_from_day(INITIAL_NPC_BIRTH_DAY)),
            position: NpcPosition::new(coord),
            velocity: Velocity::ZERO,
            max_velocity: MaxVelocity::default(),
            movement_facing: MovementFacing::default(),
            hunger: NpcHunger::fed(),
            inventory: NpcInventory::empty(),
        }
    }
}

pub fn spawn_initial_default_npc(mut commands: Commands, grid: Res<Grid>) {
    let Some(coord) = center_coord(&grid) else {
        return;
    };

    commands.spawn(InitialNpcBundle::new(coord));
}

pub fn update_npc_hunger(
    tick_duration: Res<SimulationTickDuration>,
    mut npcs: Query<(&mut NpcHunger, &mut NpcInventory), With<Npc>>,
) {
    for (mut hunger, mut inventory) in &mut npcs {
        hunger.advance_by(tick_duration.duration(), &mut inventory);
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
}
