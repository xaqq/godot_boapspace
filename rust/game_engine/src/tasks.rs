use crate::ai::{AiConstructBuilding, AiSearchForFood};
use crate::buildings::{BuildingBlueprint, ConstructionProgress};
use crate::components::{MovementTarget, Npc, NpcPosition};
use crate::navigation::{current_navigation_snapshot, NavigationSnapshot, NpcRoute};
use crate::roads::RoadBlueprint;
use crate::work::NpcWorkState;
use bevy_ecs::prelude::*;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Task;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct TaskAssignment {
    worker: Option<Entity>,
}

impl TaskAssignment {
    pub const fn unassigned() -> Self {
        Self { worker: None }
    }

    pub const fn worker(self) -> Option<Entity> {
        self.worker
    }

    pub const fn assign(&mut self, worker: Entity) {
        self.worker = Some(worker);
    }

    pub const fn clear(&mut self) {
        self.worker = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct ProgressBuildingConstruction {
    blueprint: Entity,
}

impl ProgressBuildingConstruction {
    pub const fn new(blueprint: Entity) -> Self {
        Self { blueprint }
    }

    pub const fn blueprint(self) -> Entity {
        self.blueprint
    }

    pub const fn label() -> &'static str {
        "ProgressBuildingConstruction"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Bundle)]
pub struct ProgressBuildingConstructionTaskBundle {
    task: Task,
    construction: ProgressBuildingConstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiConstructionLabor {
    site: Entity,
    interaction_cell: crate::grid::CellCoord,
}

impl AiConstructionLabor {
    pub const fn new(site: Entity, interaction_cell: crate::grid::CellCoord) -> Self {
        Self {
            site,
            interaction_cell,
        }
    }

    pub const fn site(self) -> Entity {
        self.site
    }

    pub const fn interaction_cell(self) -> crate::grid::CellCoord {
        self.interaction_cell
    }
}

impl ProgressBuildingConstructionTaskBundle {
    pub const fn new(blueprint: Entity) -> Self {
        Self {
            task: Task,
            construction: ProgressBuildingConstruction::new(blueprint),
        }
    }
}

pub fn maintain_construction_tasks(
    mut commands: Commands,
    blueprints: Query<Entity, Or<(With<BuildingBlueprint>, With<RoadBlueprint>)>>,
    tasks: Query<(Entity, &ProgressBuildingConstruction)>,
) {
    let blueprint_entities = blueprints.iter().collect::<HashSet<_>>();
    let mut represented_blueprints = HashSet::new();

    for (task_entity, task) in &tasks {
        let blueprint = task.blueprint();
        if !blueprint_entities.contains(&blueprint) || !represented_blueprints.insert(blueprint) {
            commands.entity(task_entity).despawn();
        }
    }

    for blueprint in blueprint_entities {
        if !represented_blueprints.contains(&blueprint) {
            commands.spawn(ProgressBuildingConstructionTaskBundle::new(blueprint));
        }
    }
}

/// Routes idle NPCs to material-complete construction sites and advances one
/// labor tick per worker that has reached a distinct interaction cell. Labor
/// lives on the site, so interruptions do not lose progress.
pub fn manage_construction_labor(world: &mut World) {
    let Some(snapshot) = current_navigation_snapshot(world) else {
        return;
    };

    let mut active_query = world.query::<(Entity, &NpcPosition, &AiConstructionLabor)>();
    let mut active = active_query
        .iter(world)
        .map(|(worker, position, labor)| (worker, *position, *labor))
        .collect::<Vec<_>>();
    active.sort_unstable_by_key(|(worker, ..)| worker.to_bits());
    let mut claimed_slots = HashSet::new();
    let mut completed_sites = HashSet::new();

    for (worker, position, labor) in active {
        let site = labor.site();
        let Some((cost, goals)) = construction_site(world, &snapshot, site) else {
            clear_labor(world, worker);
            continue;
        };
        let interrupted = world.get::<AiSearchForFood>(worker).is_some()
            || world.get::<AiConstructBuilding>(worker).is_none();
        let actionable = world
            .get::<ConstructionProgress>(site)
            .is_some_and(|progress| {
                progress.materials_complete(cost) && !progress.is_complete(cost)
            });
        let interaction_cell = labor.interaction_cell();
        if interrupted
            || !actionable
            || !goals.contains(&interaction_cell)
            || !claimed_slots.insert((site, interaction_cell))
        {
            clear_labor(world, worker);
            continue;
        }

        if position.coord == interaction_cell {
            world
                .entity_mut(worker)
                .remove::<NpcRoute>()
                .remove::<MovementTarget>();
            if let Some(mut progress) = world.get_mut::<ConstructionProgress>(site) {
                progress.advance_labor();
                if progress.is_complete(cost) {
                    completed_sites.insert(site);
                }
            }
        } else {
            set_route(world, worker, vec![interaction_cell]);
        }
    }

    // A worker processed earlier in entity order may have supplied the final
    // labor unit. Release every worker from a site that completed this tick.
    if !completed_sites.is_empty() {
        let mut completed_workers_query = world.query::<(Entity, &AiConstructionLabor)>();
        let completed_workers = completed_workers_query
            .iter(world)
            .filter_map(|(worker, labor)| completed_sites.contains(&labor.site()).then_some(worker))
            .collect::<Vec<_>>();
        for worker in completed_workers {
            clear_labor(world, worker);
        }
    }

    let mut sites_query = world.query::<(
        Entity,
        &ConstructionProgress,
        Option<&BuildingBlueprint>,
        Option<&RoadBlueprint>,
    )>();
    let mut available_slots = sites_query
        .iter(world)
        .filter_map(|(entity, progress, building, road)| {
            let cost = building
                .map(|blueprint| blueprint.kind.definition().construction_cost())
                .or_else(|| road.map(|blueprint| blueprint.target_tier.material_cost()))?;
            (progress.materials_complete(cost) && !progress.is_complete(cost)).then_some(entity)
        })
        .flat_map(|site| {
            construction_site(world, &snapshot, site)
                .map(|(_, cells)| {
                    cells
                        .into_iter()
                        .filter(|cell| !claimed_slots.contains(&(site, *cell)))
                        .map(|cell| (site, cell))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    available_slots.sort_unstable_by_key(|(site, cell)| (site.to_bits(), cell.y(), cell.x()));

    if available_slots.is_empty() {
        return;
    }

    let mut npc_query = world.query_filtered::<(Entity, &NpcPosition, NpcWorkState), With<Npc>>();
    let mut workers = npc_query
        .iter(world)
        .filter(|(_, _, work)| !work.is_assigned())
        .map(|(entity, position, _)| (entity, *position))
        .collect::<Vec<_>>();
    workers.sort_unstable_by_key(|(entity, _)| entity.to_bits());

    for (worker, position) in workers {
        if available_slots.is_empty() {
            break;
        }
        let Some(distances) = snapshot.distances_from(position.coord) else {
            continue;
        };
        let selected = available_slots
            .iter()
            .enumerate()
            .filter_map(|(index, (site, cell))| {
                let (_, cost) = distances.closest_reachable([*cell])?;
                Some((cost, site.to_bits(), cell.y(), cell.x(), index))
            })
            .min_by_key(|candidate| (candidate.0, candidate.1, candidate.2, candidate.3));
        let Some((_, _, _, _, selected_index)) = selected else {
            continue;
        };
        let (site, interaction_cell) = available_slots.swap_remove(selected_index);
        world.entity_mut(worker).insert((
            AiConstructBuilding::new(site),
            AiConstructionLabor::new(site, interaction_cell),
        ));
        if position.coord != interaction_cell {
            set_route(world, worker, vec![interaction_cell]);
        }
    }
}

fn construction_site(
    world: &World,
    snapshot: &NavigationSnapshot,
    site: Entity,
) -> Option<(
    crate::resources::ResourceAmounts,
    Vec<crate::grid::CellCoord>,
)> {
    if let Some(blueprint) = world.get::<BuildingBlueprint>(site).copied() {
        let goals = if matches!(
            blueprint.kind,
            crate::buildings::BuildingKind::Field | crate::buildings::BuildingKind::TreePlot
        ) {
            snapshot.footprint_interaction_cells(blueprint.footprint)
        } else {
            snapshot.exterior_interaction_cells(blueprint.footprint)
        };
        return Some((blueprint.kind.definition().construction_cost(), goals));
    }
    let blueprint = world.get::<RoadBlueprint>(site).copied()?;
    let goals = snapshot
        .is_walkable(blueprint.coord)
        .then_some(vec![blueprint.coord])
        .unwrap_or_default();
    Some((blueprint.target_tier.material_cost(), goals))
}

fn set_route(world: &mut World, worker: Entity, goals: Vec<crate::grid::CellCoord>) {
    if !world
        .get::<NpcRoute>(worker)
        .is_some_and(|route| route.goals() == goals.as_slice())
    {
        world.entity_mut(worker).insert(NpcRoute::new(goals));
        world.entity_mut(worker).remove::<MovementTarget>();
    }
}

fn clear_labor(world: &mut World, worker: Entity) {
    world
        .entity_mut(worker)
        .remove::<AiConstructionLabor>()
        .remove::<AiConstructBuilding>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>();
}
