use crate::ai::{
    system_assign_construction_work, system_deposit_construction_resources, system_gather_resource,
    system_keep_enough_food_in_inventory, system_npc_idle, system_route_construction_work,
    system_search_for_food,
};
use crate::buildings::system_complete_building_construction;
use crate::movement::update_npc_movement;
use crate::npcs::update_npc_hunger;
use crate::tasks::maintain_construction_tasks;
use bevy_ecs::prelude::IntoScheduleConfigs;
use bevy_ecs::schedule::Schedule;

pub fn build_surface_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems(
        (
            maintain_construction_tasks,
            system_keep_enough_food_in_inventory,
            system_search_for_food,
            system_assign_construction_work,
            system_route_construction_work,
            system_npc_idle,
            update_npc_movement,
            system_gather_resource,
            system_deposit_construction_resources,
            system_complete_building_construction,
            update_npc_hunger,
            maintain_construction_tasks,
        )
            .chain(),
    );
    schedule
}
