use crate::entity_id::BridgeEntityId;
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::buildings::{Building, BuildingBlueprint, ConstructionProgress};
use game_engine::farming::{HarvestField, SeedField};
use game_engine::forestry::{CutTreePlot, SeedTreePlot};
use game_engine::npcs::NpcName;
use game_engine::refining::{
    AiRefineResource, RefineryProduction, RefiningTask, REFINING_TICKS_PER_UNIT,
};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::roads::RoadBlueprint;
use game_engine::tasks::{ProgressBuildingConstruction, TaskAssignment};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaskTableRow {
    pub(super) entity_id: BridgeEntityId,
    pub(super) task_type: String,
    pub(super) assignment: String,
    pub(super) worker: String,
    pub(super) building: String,
    pub(super) recipe: String,
    pub(super) progress: String,
}

pub(super) fn query_task_table_rows(world: &World) -> Vec<TaskTableRow> {
    let mut rows = world
        .try_query::<(Entity, &ProgressBuildingConstruction)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter_map(|(entity, construction)| {
                    let entity_id = BridgeEntityId::try_from(entity).ok()?;
                    Some(TaskTableRow {
                        entity_id,
                        task_type: ProgressBuildingConstruction::label().to_string(),
                        assignment: em_dash(),
                        worker: em_dash(),
                        building: format_construction_task_building(
                            world,
                            construction.blueprint(),
                        ),
                        recipe: em_dash(),
                        progress: format_construction_task_progress(
                            world,
                            construction.blueprint(),
                        ),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if let Some(mut query) = world.try_query::<(Entity, &SeedField)>() {
        rows.extend(query.iter(world).filter_map(|(entity, seed)| {
            let entity_id = BridgeEntityId::try_from(entity).ok()?;
            Some(TaskTableRow {
                entity_id,
                task_type: SeedField::label().to_string(),
                assignment: em_dash(),
                worker: em_dash(),
                building: format_plot_task_building(world, "Field", seed.field()),
                recipe: em_dash(),
                progress: em_dash(),
            })
        }));
    }

    if let Some(mut query) = world.try_query::<(Entity, &HarvestField)>() {
        rows.extend(query.iter(world).filter_map(|(entity, harvest)| {
            let entity_id = BridgeEntityId::try_from(entity).ok()?;
            Some(TaskTableRow {
                entity_id,
                task_type: HarvestField::label().to_string(),
                assignment: em_dash(),
                worker: em_dash(),
                building: format_plot_task_building(world, "Field", harvest.field()),
                recipe: em_dash(),
                progress: em_dash(),
            })
        }));
    }

    if let Some(mut query) = world.try_query::<(Entity, &SeedTreePlot)>() {
        rows.extend(query.iter(world).filter_map(|(entity, seed)| {
            let entity_id = BridgeEntityId::try_from(entity).ok()?;
            Some(TaskTableRow {
                entity_id,
                task_type: SeedTreePlot::label().to_string(),
                assignment: em_dash(),
                worker: em_dash(),
                building: format_plot_task_building(world, "Tree Plot", seed.tree_plot()),
                recipe: em_dash(),
                progress: em_dash(),
            })
        }));
    }

    if let Some(mut query) = world.try_query::<(Entity, &CutTreePlot)>() {
        rows.extend(query.iter(world).filter_map(|(entity, cut)| {
            let entity_id = BridgeEntityId::try_from(entity).ok()?;
            Some(TaskTableRow {
                entity_id,
                task_type: CutTreePlot::label().to_string(),
                assignment: em_dash(),
                worker: em_dash(),
                building: format_plot_task_building(world, "Tree Plot", cut.tree_plot()),
                recipe: em_dash(),
                progress: em_dash(),
            })
        }));
    }

    if let Some(mut query) = world.try_query::<(Entity, &RefiningTask, Option<&TaskAssignment>)>() {
        rows.extend(
            query
                .iter(world)
                .filter_map(|(entity, refining, assignment)| {
                    let entity_id = BridgeEntityId::try_from(entity).ok()?;
                    Some(refining_task_table_row(
                        world,
                        entity_id,
                        refining.refinery(),
                        assignment
                            .copied()
                            .unwrap_or_else(TaskAssignment::unassigned),
                    ))
                }),
        );
    }

    rows.sort_by_key(|row| row.entity_id);
    rows
}

fn format_construction_task_building(world: &World, blueprint: Entity) -> String {
    let blueprint_id = BridgeEntityId::try_from(blueprint)
        .ok()
        .map(|id| id.signal_value().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let Some(blueprint_data) = world.get::<BuildingBlueprint>(blueprint) else {
        if let Some(road) = world.get::<RoadBlueprint>(blueprint) {
            return format!(
                "{} Blueprint {} at ({}, {})",
                road.target_tier.label(),
                blueprint_id,
                road.coord.x(),
                road.coord.y()
            );
        }
        return format!("Blueprint {blueprint_id}: unavailable");
    };

    let origin = blueprint_data.footprint.origin();
    format!(
        "{} Blueprint {} at ({}, {})",
        blueprint_data.kind.label(),
        blueprint_id,
        origin.x(),
        origin.y()
    )
}

fn format_construction_task_progress(world: &World, blueprint: Entity) -> String {
    let Some(progress) = world.get::<ConstructionProgress>(blueprint) else {
        return em_dash();
    };
    let cost = world
        .get::<BuildingBlueprint>(blueprint)
        .map(|data| data.kind.definition().construction_cost())
        .or_else(|| {
            world
                .get::<RoadBlueprint>(blueprint)
                .map(|data| data.target_tier.material_cost())
        });
    let Some(cost) = cost else {
        return em_dash();
    };
    format!(
        "{} | Labor {}/{}",
        format_deposited_over_required(progress.deposited(), cost),
        progress.labor_completed(),
        progress.labor_required()
    )
}

fn format_plot_task_building(world: &World, label: &str, plot: Entity) -> String {
    let plot_id = BridgeEntityId::try_from(plot)
        .ok()
        .map(|id| id.signal_value().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let Some(building) = world.get::<Building>(plot) else {
        return format!("{label} {plot_id}: unavailable");
    };
    let origin = building.footprint.origin();
    format!("{label} {plot_id} at ({}, {})", origin.x(), origin.y())
}

fn refining_task_table_row(
    world: &World,
    entity_id: BridgeEntityId,
    refinery: Entity,
    assignment: TaskAssignment,
) -> TaskTableRow {
    let assigned_worker = assignment.worker();
    let production = world
        .get::<RefineryProduction>(refinery)
        .copied()
        .unwrap_or_default();
    let recipe = production.recipe().or_else(|| {
        assigned_worker
            .and_then(|worker| world.get::<AiRefineResource>(worker))
            .map(|work| work.recipe())
    });
    let building = world
        .get::<Building>(refinery)
        .map_or_else(em_dash, |building| {
            let id = BridgeEntityId::try_from(refinery)
                .ok()
                .map(|id| id.signal_value().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            format!("{} {id}", building.kind.label())
        });
    let progress = if production.recipe().is_some() {
        format!(
            "{}/{} ({} remaining)",
            production.progress_ticks(),
            REFINING_TICKS_PER_UNIT,
            production.remaining_ticks()
        )
    } else {
        em_dash()
    };

    TaskTableRow {
        entity_id,
        task_type: RefiningTask::label().to_string(),
        assignment: if assigned_worker.is_some() {
            "Assigned".to_string()
        } else {
            "Unassigned".to_string()
        },
        worker: assigned_worker.map_or_else(em_dash, |worker| format_worker(world, worker)),
        building,
        recipe: recipe.map_or_else(em_dash, |recipe| recipe.label().to_string()),
        progress,
    }
}

fn format_worker(world: &World, worker: Entity) -> String {
    let id = BridgeEntityId::try_from(worker)
        .ok()
        .map(|id| id.signal_value().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    world.get::<NpcName>(worker).map_or_else(
        || format!("NPC {id}"),
        |name| format!("{} ({id})", name.as_str()),
    )
}

fn em_dash() -> String {
    "—".to_string()
}

fn format_deposited_over_required(progress: ResourceAmounts, cost: ResourceAmounts) -> String {
    let parts = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let required = cost.get(kind);
            (required > 0).then(|| format!("{}: {}/{}", kind.label(), progress.get(kind), required))
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        "None".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::buildings::{BuildingBlueprintBundle, BuildingFootprint, BuildingKind};
    use game_engine::farming::{FarmInventory, FieldCrop, FieldOwner};
    use game_engine::forestry::{ForesterLodgeInventory, TreePlotGrowth, TreePlotOwner};
    use game_engine::grid::CellCoord;
    use game_engine::tasks::{ProgressBuildingConstructionTaskBundle, Task};

    #[test]
    fn task_table_rows_format_construction_tasks() {
        let mut world = World::new();
        let blueprint = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(4, 7), 2, 2),
            ))
            .id();
        let task = world
            .spawn(ProgressBuildingConstructionTaskBundle::new(blueprint))
            .id();

        let rows = query_task_table_rows(&world);

        assert_eq!(
            rows,
            vec![TaskTableRow {
                entity_id: BridgeEntityId::try_from(task).expect("task entity id should encode"),
                task_type: "ProgressBuildingConstruction".to_string(),
                assignment: em_dash(),
                worker: em_dash(),
                building: format!(
                    "Warehouse Blueprint {} at (4, 7)",
                    BridgeEntityId::try_from(blueprint)
                        .expect("blueprint entity id should encode")
                        .signal_value()
                ),
                recipe: em_dash(),
                progress: "Planks: 0/40, Stone Blocks: 0/20 | Labor 0/720".to_string(),
            }]
        );
    }

    #[test]
    fn task_table_rows_format_farming_tasks() {
        let mut world = World::new();
        let farm = world
            .spawn((
                Building::new(
                    BuildingKind::Farm,
                    BuildingFootprint::new(CellCoord::new(0, 0), 3, 3),
                ),
                FarmInventory::empty(),
            ))
            .id();
        let field = world
            .spawn((
                Building::new(
                    BuildingKind::Field,
                    BuildingFootprint::new(CellCoord::new(3, 1), 1, 1),
                ),
                FieldOwner::new(farm),
                FieldCrop::seedable(),
            ))
            .id();
        let seed_task = world.spawn((Task, SeedField::new(field))).id();
        let harvest_task = world.spawn((Task, HarvestField::new(field))).id();

        let rows = query_task_table_rows(&world);

        assert_eq!(rows.len(), 2);
        assert!(rows.contains(&TaskTableRow {
            entity_id: BridgeEntityId::try_from(seed_task).expect("task entity id should encode"),
            task_type: "SeedField".to_string(),
            assignment: em_dash(),
            worker: em_dash(),
            building: format!(
                "Field {} at (3, 1)",
                BridgeEntityId::try_from(field)
                    .expect("field entity id should encode")
                    .signal_value()
            ),
            recipe: em_dash(),
            progress: em_dash(),
        }));
        assert!(rows.contains(&TaskTableRow {
            entity_id: BridgeEntityId::try_from(harvest_task)
                .expect("task entity id should encode"),
            task_type: "HarvestField".to_string(),
            assignment: em_dash(),
            worker: em_dash(),
            building: format!(
                "Field {} at (3, 1)",
                BridgeEntityId::try_from(field)
                    .expect("field entity id should encode")
                    .signal_value()
            ),
            recipe: em_dash(),
            progress: em_dash(),
        }));
    }

    #[test]
    fn task_table_rows_format_forestry_tasks() {
        let mut world = World::new();
        let lodge = world
            .spawn((
                Building::new(
                    BuildingKind::ForesterLodge,
                    BuildingFootprint::new(CellCoord::new(0, 0), 3, 3),
                ),
                ForesterLodgeInventory::empty(),
            ))
            .id();
        let tree_plot = world
            .spawn((
                Building::new(
                    BuildingKind::TreePlot,
                    BuildingFootprint::new(CellCoord::new(3, 1), 1, 1),
                ),
                TreePlotOwner::new(lodge),
                TreePlotGrowth::seedable(),
            ))
            .id();
        let seed_task = world.spawn((Task, SeedTreePlot::new(tree_plot))).id();
        let cut_task = world.spawn((Task, CutTreePlot::new(tree_plot))).id();

        let rows = query_task_table_rows(&world);

        assert_eq!(rows.len(), 2);
        assert!(rows.contains(&TaskTableRow {
            entity_id: BridgeEntityId::try_from(seed_task).expect("task entity id should encode"),
            task_type: "SeedTreePlot".to_string(),
            assignment: em_dash(),
            worker: em_dash(),
            building: format!(
                "Tree Plot {} at (3, 1)",
                BridgeEntityId::try_from(tree_plot)
                    .expect("tree plot entity id should encode")
                    .signal_value()
            ),
            recipe: em_dash(),
            progress: em_dash(),
        }));
        assert!(rows.contains(&TaskTableRow {
            entity_id: BridgeEntityId::try_from(cut_task).expect("task entity id should encode"),
            task_type: "CutTreePlot".to_string(),
            assignment: em_dash(),
            worker: em_dash(),
            building: format!(
                "Tree Plot {} at (3, 1)",
                BridgeEntityId::try_from(tree_plot)
                    .expect("tree plot entity id should encode")
                    .signal_value()
            ),
            recipe: em_dash(),
            progress: em_dash(),
        }));
    }

    #[test]
    fn task_table_rows_include_refining_assignment_and_building() {
        let mut world = World::new();
        let refinery = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(4, 7), 2, 2),
                ),
                RefineryProduction::default(),
            ))
            .id();
        let task = world
            .spawn((
                Task,
                TaskAssignment::unassigned(),
                RefiningTask::new(refinery),
            ))
            .id();

        assert_eq!(
            query_task_table_rows(&world),
            vec![TaskTableRow {
                entity_id: BridgeEntityId::try_from(task).expect("task entity id should encode"),
                task_type: "RefineResource".to_string(),
                assignment: "Unassigned".to_string(),
                worker: em_dash(),
                building: format!(
                    "Sawmill {}",
                    BridgeEntityId::try_from(refinery)
                        .expect("refinery id should encode")
                        .signal_value()
                ),
                recipe: em_dash(),
                progress: em_dash(),
            }]
        );
    }

    #[test]
    fn mixed_task_rows_are_sorted_by_entity_id_not_query_group() {
        let mut world = World::new();
        let blueprint = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(4, 7), 2, 2),
            ))
            .id();
        let construction_task = world
            .spawn(ProgressBuildingConstructionTaskBundle::new(blueprint))
            .id();
        let refinery = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(0, 0), 2, 2),
                ),
                RefineryProduction::default(),
            ))
            .id();
        let refining_task = world
            .spawn((
                Task,
                TaskAssignment::unassigned(),
                RefiningTask::new(refinery),
            ))
            .id();

        let refining_id =
            BridgeEntityId::try_from(refining_task).expect("refining task ID should encode");
        let construction_id = BridgeEntityId::try_from(construction_task)
            .expect("construction task ID should encode");
        assert!(
            refining_id < construction_id,
            "test fixture must invert query order"
        );

        let rows = query_task_table_rows(&world);

        assert_eq!(
            rows.iter().map(|row| row.entity_id).collect::<Vec<_>>(),
            vec![refining_id, construction_id]
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.task_type.as_str())
                .collect::<Vec<_>>(),
            vec!["RefineResource", "ProgressBuildingConstruction"]
        );
    }
}
