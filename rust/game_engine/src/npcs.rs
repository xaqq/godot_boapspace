pub use crate::components::{BirthDate, Npc, NpcName, NpcPosition};

use crate::grid::{CellCoord, Grid};
use bevy_ecs::prelude::*;

pub const INITIAL_NPC_NAME: &str = "Mara Voss";
pub const INITIAL_NPC_BIRTH_DAY: i32 = 320;
pub const DEFAULT_WORLD_DAY: i32 = 12_000;
const DAYS_PER_YEAR: i32 = 365;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct WorldDay {
    day: i32,
}

impl WorldDay {
    pub const fn new(day: i32) -> Self {
        Self { day }
    }

    pub const fn day(self) -> i32 {
        self.day
    }

    pub fn age_years_since(self, birth_date: BirthDate) -> u32 {
        let lived_days = self.day.checked_sub(birth_date.day()).unwrap_or(0).max(0);
        u32::try_from(lived_days / DAYS_PER_YEAR).unwrap_or(0)
    }
}

pub fn spawn_initial_default_npc(mut commands: Commands, grid: Res<Grid>) {
    let Some(coord) = center_coord(&grid) else {
        return;
    };

    commands.spawn((
        Npc,
        NpcName::new(INITIAL_NPC_NAME),
        BirthDate::new(INITIAL_NPC_BIRTH_DAY),
        NpcPosition { coord },
    ));
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
    fn test_world_day_age_years_since_birth_date() {
        let world_day = WorldDay::new(DEFAULT_WORLD_DAY);

        assert_eq!(
            world_day.age_years_since(BirthDate::new(INITIAL_NPC_BIRTH_DAY)),
            32
        );
    }

    #[test]
    fn test_world_day_does_not_return_negative_age() {
        let world_day = WorldDay::new(10);

        assert_eq!(world_day.age_years_since(BirthDate::new(20)), 0);
    }
}
