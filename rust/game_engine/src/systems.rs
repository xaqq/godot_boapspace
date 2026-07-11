use crate::ai::{system_assign_plot_work, system_npc_idle, system_route_plot_work};
use crate::buildings::system_complete_building_construction;
use crate::farming::{
    maintain_farming_tasks, system_advance_field_growth, system_harvest_fields, system_seed_fields,
};
use crate::forestry::{
    maintain_forestry_tasks, system_advance_tree_growth, system_cut_tree_plots,
    system_seed_tree_plots,
};
use crate::housing::maintain_housing_assignments;
use crate::logistics::{manage_construction_logistics, manage_food_logistics};
use crate::movement::update_npc_movement;
use crate::navigation::{drive_npc_routes, refresh_navigation_snapshot};
use crate::npcs::update_npc_hunger;
use crate::refining::{
    assign_refining_work, maintain_refining_tasks, route_and_advance_refining_work,
};
use crate::tasks::maintain_construction_tasks;
use bevy_ecs::prelude::IntoScheduleConfigs;
use bevy_ecs::schedule::Schedule;

pub fn build_surface_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems(
        (
            (
                maintain_construction_tasks,
                refresh_navigation_snapshot,
                maintain_farming_tasks,
                maintain_forestry_tasks,
                maintain_refining_tasks,
                system_advance_field_growth,
                system_advance_tree_growth,
                manage_food_logistics,
                manage_construction_logistics,
                assign_refining_work,
                route_and_advance_refining_work,
                system_assign_plot_work,
                system_route_plot_work,
                system_npc_idle,
            )
                .chain(),
            (
                drive_npc_routes,
                update_npc_movement,
                system_seed_fields,
                system_harvest_fields,
                system_seed_tree_plots,
                system_cut_tree_plots,
                system_complete_building_construction,
                maintain_housing_assignments,
                update_npc_hunger,
                maintain_construction_tasks,
                maintain_farming_tasks,
                maintain_forestry_tasks,
                maintain_refining_tasks,
            )
                .chain(),
        )
            .chain(),
    );
    schedule
}
