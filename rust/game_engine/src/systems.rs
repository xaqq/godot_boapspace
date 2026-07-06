use crate::movement::update_npc_movement;
use crate::npcs::update_npc_hunger;
use crate::tasks::maintain_construction_tasks;
use bevy_ecs::schedule::Schedule;

pub fn build_surface_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems((
        update_npc_movement,
        update_npc_hunger,
        maintain_construction_tasks,
    ));
    schedule
}
