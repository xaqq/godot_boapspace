use crate::tasks::maintain_construction_tasks;
use bevy_ecs::schedule::Schedule;

pub fn build_surface_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems(maintain_construction_tasks);
    schedule
}
