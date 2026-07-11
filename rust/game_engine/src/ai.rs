pub use crate::components::{
    AiConstructBuilding, AiGatherResource, AiIdleRoam, AiKeepEnoughFoodInInventory, AiSearchForFood,
};

use crate::buildings::{Building, BuildingBlueprint, BuildingFootprint, ConstructionProgress};
use crate::components::{
    MovementTarget, Npc, NpcInventory, NpcPosition, ResourceNode, TilePosition,
};
use crate::farming::{
    field_harvest_is_actionable, field_seeding_is_actionable, AiHarvestField, AiSeedField,
    FarmInventory, Farmer, FieldCrop, FieldOwner, HarvestField, SeedField,
};
use crate::forestry::{
    tree_plot_cutting_is_actionable, tree_plot_seeding_is_actionable, AiCutTreePlot,
    AiSeedTreePlot, CutTreePlot, Forester, ForesterLodgeInventory, SeedTreePlot, TreePlotGrowth,
    TreePlotOwner,
};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::housing::{House, HousingAssignment};
use crate::logistics::AiConstructionHaul;
use crate::navigation::{NavigationSnapshot, NpcRoute};
use crate::resources::{ResourceAmounts, ResourceKind};
use crate::skills::{NpcSkills, SkillKind};
use crate::tasks::ProgressBuildingConstruction;
use crate::work::NpcWorkState;
use bevy_ecs::prelude::*;

pub const DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD: u32 = 5;
pub const DEFAULT_NPC_FOOD_INVENTORY_TARGET: u32 = 100;
pub const DEFAULT_NPC_IDLE_ROAM_RADIUS: u32 = 3;
pub const DEFAULT_NPC_IDLE_DWELL_TICKS: u32 = 180;
pub const RESOURCE_GATHER_TICKS_PER_UNIT: u32 = 60;
pub const CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE: u32 = 10;

const IDLE_ROAM_OFFSETS: [(i32, i32); 24] = [
    (1, 0),
    (0, 1),
    (-1, 0),
    (0, -1),
    (2, 0),
    (1, 1),
    (0, 2),
    (-1, 1),
    (-2, 0),
    (-1, -1),
    (0, -2),
    (1, -1),
    (3, 0),
    (2, 1),
    (1, 2),
    (0, 3),
    (-1, 2),
    (-2, 1),
    (-3, 0),
    (-2, -1),
    (-1, -2),
    (0, -3),
    (1, -2),
    (2, -1),
];

pub fn system_keep_enough_food_in_inventory(
    mut commands: Commands,
    npcs: Query<
        (
            Entity,
            &NpcInventory,
            &AiKeepEnoughFoodInInventory,
            Option<&AiSearchForFood>,
        ),
        With<Npc>,
    >,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    let food_available = resource_kind_available(ResourceKind::Food, &resource_nodes);
    for (entity, inventory, goal, search) in &npcs {
        if search.is_some() {
            if !should_continue_food_refill(inventory, goal) || !food_available {
                commands.entity(entity).remove::<AiSearchForFood>();
                commands.entity(entity).remove::<MovementTarget>();
            }
            continue;
        }

        if should_start_food_refill(inventory, goal) && food_available {
            commands.entity(entity).insert(AiSearchForFood);
        }
    }
}

pub fn system_search_for_food(
    mut commands: Commands,
    npcs: Query<
        (
            Entity,
            &NpcPosition,
            &NpcInventory,
            &AiKeepEnoughFoodInInventory,
            Option<&AiGatherResource>,
            Option<&MovementTarget>,
        ),
        (With<Npc>, With<AiSearchForFood>),
    >,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    for (entity, position, inventory, goal, gather, movement_target) in &npcs {
        if !should_continue_food_refill(inventory, goal) {
            commands.entity(entity).remove::<AiSearchForFood>();
            commands.entity(entity).remove::<MovementTarget>();
            continue;
        }

        let Some((resource_entity, resource_coord)) =
            nearest_food_resource(position.coord, &resource_nodes)
        else {
            commands.entity(entity).remove::<AiSearchForFood>();
            commands.entity(entity).remove::<MovementTarget>();
            continue;
        };

        if is_cardinally_adjacent(position.coord, resource_coord) {
            if movement_target.is_some() {
                commands.entity(entity).remove::<MovementTarget>();
            }
            if gather.map(|gather| gather.target()) != Some(resource_entity) {
                commands
                    .entity(entity)
                    .insert(AiGatherResource::new(resource_entity));
            }
            continue;
        }

        if movement_target.map(|target| target.coord) != Some(resource_coord) {
            commands.entity(entity).insert((
                NpcRoute::new(cardinal_interaction_cells(resource_coord)),
                MovementTarget::new(resource_coord),
            ));
        }
    }
}

pub fn system_assign_construction_work(
    mut commands: Commands,
    npcs: Query<
        (
            Entity,
            &NpcPosition,
            &NpcInventory,
            Option<&AiKeepEnoughFoodInInventory>,
            NpcWorkState,
        ),
        With<Npc>,
    >,
    tasks: Query<&ProgressBuildingConstruction>,
    blueprints: Query<(Entity, &BuildingBlueprint, &ConstructionProgress)>,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    for (entity, position, inventory, keep_food, work) in &npcs {
        if work.is_assigned() || should_interrupt_for_food(inventory, keep_food, &resource_nodes) {
            continue;
        }

        let Some(blueprint) = construction_work_target(
            position.coord,
            inventory,
            &tasks,
            &blueprints,
            &resource_nodes,
        ) else {
            continue;
        };

        commands
            .entity(entity)
            .insert(AiConstructBuilding::new(blueprint));
    }
}

pub fn system_assign_plot_work(
    mut commands: Commands,
    npcs: Query<
        (
            Entity,
            &NpcPosition,
            Option<&Farmer>,
            Option<&Forester>,
            NpcWorkState,
        ),
        With<Npc>,
    >,
    seed_tasks: Query<&SeedField>,
    harvest_tasks: Query<&HarvestField>,
    tree_seed_tasks: Query<&SeedTreePlot>,
    tree_cut_tasks: Query<&CutTreePlot>,
    active_seed_work: Query<&AiSeedField>,
    active_harvest_work: Query<&AiHarvestField>,
    active_tree_seed_work: Query<&AiSeedTreePlot>,
    active_tree_cut_work: Query<&AiCutTreePlot>,
    fields: Query<(Entity, &Building, &FieldOwner, &FieldCrop)>,
    farms: Query<(&Building, &FarmInventory)>,
    tree_plots: Query<(Entity, &Building, &TreePlotOwner, &TreePlotGrowth)>,
    lodges: Query<(&Building, &ForesterLodgeInventory)>,
) {
    let mut claimed_fields = active_seed_work
        .iter()
        .map(|seed| seed.field())
        .collect::<std::collections::HashSet<_>>();
    claimed_fields.extend(active_harvest_work.iter().map(|harvest| harvest.field()));
    let mut claimed_tree_plots = active_tree_seed_work
        .iter()
        .map(|seed| seed.tree_plot())
        .collect::<std::collections::HashSet<_>>();
    claimed_tree_plots.extend(active_tree_cut_work.iter().map(|cut| cut.tree_plot()));

    let mut eligible_npcs = npcs.iter().collect::<Vec<_>>();
    eligible_npcs.sort_by_key(|(entity, ..)| entity.to_bits());
    for (entity, position, farmer, forester, work) in eligible_npcs {
        if work.is_assigned() {
            continue;
        }

        let Some(target) = plot_work_target(
            position.coord,
            farmer.is_some(),
            forester.is_some(),
            &seed_tasks,
            &harvest_tasks,
            &tree_seed_tasks,
            &tree_cut_tasks,
            &claimed_fields,
            &claimed_tree_plots,
            &fields,
            &farms,
            &tree_plots,
            &lodges,
        ) else {
            continue;
        };

        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<AiIdleRoam>();
        match target {
            PlotWorkTarget::SeedField(field) => {
                claimed_fields.insert(field);
                entity_commands.insert(AiSeedField::new(field));
            }
            PlotWorkTarget::HarvestField(field) => {
                claimed_fields.insert(field);
                entity_commands.insert(AiHarvestField::new(field));
            }
            PlotWorkTarget::SeedTreePlot(tree_plot) => {
                claimed_tree_plots.insert(tree_plot);
                entity_commands.insert(AiSeedTreePlot::new(tree_plot));
            }
            PlotWorkTarget::CutTreePlot(tree_plot) => {
                claimed_tree_plots.insert(tree_plot);
                entity_commands.insert(AiCutTreePlot::new(tree_plot));
            }
        }
    }
}

pub use system_assign_plot_work as system_assign_farming_work;

pub fn system_route_plot_work(
    mut commands: Commands,
    npcs: Query<
        (
            Entity,
            &NpcPosition,
            Option<&AiSearchForFood>,
            Option<&AiGatherResource>,
            Option<&AiConstructBuilding>,
            Option<&AiSeedField>,
            Option<&AiHarvestField>,
            Option<&AiSeedTreePlot>,
            Option<&AiCutTreePlot>,
            Option<&MovementTarget>,
        ),
        With<Npc>,
    >,
    fields: Query<(Entity, &Building, &FieldOwner, &FieldCrop)>,
    farms: Query<(&Building, &FarmInventory)>,
    tree_plots: Query<(Entity, &Building, &TreePlotOwner, &TreePlotGrowth)>,
    lodges: Query<(&Building, &ForesterLodgeInventory)>,
) {
    for (
        entity,
        position,
        search,
        gather,
        construction,
        field_seed,
        field_harvest,
        tree_seed,
        tree_cut,
        movement_target,
    ) in &npcs
    {
        if search.is_some() || gather.is_some() || construction.is_some() {
            continue;
        }

        if let Some(seed) = field_seed {
            let Some(coord) = field_seeding_is_actionable(seed.field(), &fields, &farms) else {
                commands.entity(entity).remove::<AiSeedField>();
                commands.entity(entity).remove::<MovementTarget>();
                continue;
            };
            route_to_farming_cell(&mut commands, entity, position, movement_target, coord);
            continue;
        }

        if let Some(harvest) = field_harvest {
            let Some(coord) = field_harvest_is_actionable(harvest.field(), &fields, &farms) else {
                commands.entity(entity).remove::<AiHarvestField>();
                commands.entity(entity).remove::<MovementTarget>();
                continue;
            };
            route_to_farming_cell(&mut commands, entity, position, movement_target, coord);
            continue;
        }

        if let Some(seed) = tree_seed {
            let Some(coord) =
                tree_plot_seeding_is_actionable(seed.tree_plot(), &tree_plots, &lodges)
            else {
                commands.entity(entity).remove::<AiSeedTreePlot>();
                commands.entity(entity).remove::<MovementTarget>();
                continue;
            };
            route_to_farming_cell(&mut commands, entity, position, movement_target, coord);
            continue;
        }

        if let Some(cut) = tree_cut {
            let Some(coord) =
                tree_plot_cutting_is_actionable(cut.tree_plot(), &tree_plots, &lodges)
            else {
                commands.entity(entity).remove::<AiCutTreePlot>();
                commands.entity(entity).remove::<MovementTarget>();
                continue;
            };
            route_to_farming_cell(&mut commands, entity, position, movement_target, coord);
        }
    }
}

pub use system_route_plot_work as system_route_farming_work;

pub fn system_route_construction_work(
    mut commands: Commands,
    mut npcs: Query<
        (
            Entity,
            &NpcPosition,
            &NpcInventory,
            &mut AiConstructBuilding,
            Option<&AiSearchForFood>,
            Option<&AiGatherResource>,
            Option<&AiConstructionHaul>,
            Option<&MovementTarget>,
        ),
        With<Npc>,
    >,
    blueprints: Query<(&BuildingBlueprint, &ConstructionProgress)>,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    for (
        entity,
        position,
        inventory,
        mut construction,
        search,
        gather,
        logistics_haul,
        movement_target,
    ) in &mut npcs
    {
        if search.is_some() || gather.is_some() || logistics_haul.is_some() {
            continue;
        }

        let Ok((blueprint, progress)) = blueprints.get(construction.blueprint()) else {
            commands.entity(entity).remove::<AiConstructBuilding>();
            commands.entity(entity).remove::<MovementTarget>();
            continue;
        };
        let cost = blueprint.kind.definition().construction_cost();
        if progress.is_complete(cost) {
            commands.entity(entity).remove::<AiConstructBuilding>();
            commands.entity(entity).remove::<MovementTarget>();
            continue;
        }

        if should_route_to_construction_deposit(
            inventory,
            &construction,
            progress,
            cost,
            &resource_nodes,
        ) {
            route_to_building_footprint(
                &mut commands,
                entity,
                position,
                movement_target,
                blueprint.footprint,
            );
            continue;
        }

        if construction
            .target_kind()
            .is_some_and(|kind| progress.remaining(cost, kind) == 0)
        {
            construction.clear_target_kind();
        }

        let target_kind = match construction.target_kind() {
            Some(kind) => kind,
            None => {
                let Some(kind) = construction_resource_target_kind(
                    position.coord,
                    progress,
                    cost,
                    &resource_nodes,
                ) else {
                    commands.entity(entity).remove::<AiConstructBuilding>();
                    commands.entity(entity).remove::<MovementTarget>();
                    continue;
                };
                construction.set_target_kind(kind);
                kind
            }
        };

        let remaining = progress.remaining(cost, target_kind);
        let batch_target = remaining.min(CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE);
        let carried = inventory.contents().get(target_kind);
        if carried >= batch_target {
            route_to_building_footprint(
                &mut commands,
                entity,
                position,
                movement_target,
                blueprint.footprint,
            );
            continue;
        }

        if inventory.free_size() == 0 {
            if carried > 0 {
                route_to_building_footprint(
                    &mut commands,
                    entity,
                    position,
                    movement_target,
                    blueprint.footprint,
                );
            } else {
                commands.entity(entity).remove::<AiConstructBuilding>();
                commands.entity(entity).remove::<MovementTarget>();
            }
            continue;
        }

        let Some((resource_entity, resource_coord)) =
            nearest_resource_node_of_kind(position.coord, target_kind, &resource_nodes)
        else {
            if carried > 0 {
                route_to_building_footprint(
                    &mut commands,
                    entity,
                    position,
                    movement_target,
                    blueprint.footprint,
                );
            } else {
                commands.entity(entity).remove::<AiConstructBuilding>();
                commands.entity(entity).remove::<MovementTarget>();
            }
            continue;
        };

        if is_cardinally_adjacent(position.coord, resource_coord) {
            if movement_target.is_some() {
                commands.entity(entity).remove::<MovementTarget>();
            }
            commands
                .entity(entity)
                .insert(AiGatherResource::new(resource_entity));
        } else if movement_target.map(|target| target.coord) != Some(resource_coord) {
            commands.entity(entity).insert((
                NpcRoute::new(cardinal_interaction_cells(resource_coord)),
                MovementTarget::new(resource_coord),
            ));
        }
    }
}

pub fn system_deposit_construction_resources(
    mut commands: Commands,
    mut npcs: Query<
        (
            Entity,
            &NpcPosition,
            &mut NpcInventory,
            &mut AiConstructBuilding,
            Option<&AiSearchForFood>,
            Option<&AiGatherResource>,
            Option<&AiConstructionHaul>,
        ),
        With<Npc>,
    >,
    mut blueprints: Query<(&BuildingBlueprint, &mut ConstructionProgress)>,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    for (entity, position, mut inventory, mut construction, search, gather, logistics_haul) in
        &mut npcs
    {
        if search.is_some() || gather.is_some() || logistics_haul.is_some() {
            continue;
        }

        let Ok((blueprint, mut progress)) = blueprints.get_mut(construction.blueprint()) else {
            commands.entity(entity).remove::<AiConstructBuilding>();
            continue;
        };
        if !building_interaction_reached(blueprint.kind, blueprint.footprint, position.coord) {
            continue;
        }

        let cost = blueprint.kind.definition().construction_cost();
        if !should_route_to_construction_deposit(
            &inventory,
            &construction,
            &progress,
            cost,
            &resource_nodes,
        ) {
            continue;
        }

        let mut deposited_any = false;
        for kind in ResourceKind::ALL {
            let carried = inventory.contents().get(kind);
            let remaining = progress.remaining(cost, kind);
            let amount = carried
                .min(remaining)
                .min(CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE);
            if amount == 0 {
                continue;
            }
            if inventory.consume(kind, amount) {
                progress.deposit(kind, amount, cost);
                deposited_any = true;
            }
        }

        if deposited_any {
            construction.clear_target_kind();
            if progress.is_complete(cost) {
                commands.entity(entity).remove::<AiConstructBuilding>();
            }
        }
    }
}

pub fn system_npc_idle(
    mut commands: Commands,
    grid: Res<Grid>,
    navigation: Res<NavigationSnapshot>,
    houses: Query<(&Building, &House)>,
    mut npcs: Query<
        (
            Entity,
            &NpcPosition,
            Option<&HousingAssignment>,
            Option<&MovementTarget>,
            NpcWorkState,
            Option<&mut AiIdleRoam>,
        ),
        With<Npc>,
    >,
) {
    let size = grid.size();
    for (entity, position, housing_assignment, movement_target, work, idle) in &mut npcs {
        if work.is_assigned() {
            commands.entity(entity).remove::<AiIdleRoam>();
            continue;
        }

        let assigned_house = housing_assignment.and_then(|assignment| {
            let (building, house) = houses.get(assignment.house()).ok()?;
            (building.kind.definition().housing_capacity().is_some()
                && building.footprint.is_within(size)
                && house.capacity() > 0
                && assignment.slot() < house.capacity())
            .then_some((
                assignment.house(),
                building.footprint,
                house.capacity(),
                assignment.slot(),
            ))
        });
        let assigned_house_slot = assigned_house.map(|(house, _, _, slot)| (house, slot));

        if idle
            .as_deref()
            .is_some_and(|idle| idle.house().zip(idle.house_slot()) != assigned_house_slot)
        {
            commands
                .entity(entity)
                .remove::<NpcRoute>()
                .remove::<MovementTarget>()
                .insert(idle_state(position.coord, assigned_house, size));
            continue;
        }

        if movement_target.is_some() {
            continue;
        }

        let Some(mut idle) = idle else {
            commands
                .entity(entity)
                .insert(idle_state(position.coord, assigned_house, size));
            continue;
        };

        if idle.dwell_ticks_remaining() > 0 {
            idle.advance_dwell();
            if idle.dwell_ticks_remaining() > 0 {
                continue;
            }
        }

        let next_target = if let Some((_, footprint, _, _)) = assigned_house {
            house_idle_roam_target(
                footprint,
                position.coord,
                size,
                idle.next_offset_index(),
                &navigation,
            )
        } else {
            idle_roam_target(
                idle.origin(),
                position.coord,
                size,
                idle.next_offset_index(),
            )
        };

        if let Some((target, next_offset_index)) = next_target {
            idle.set_next_offset_index(next_offset_index);
            commands
                .entity(entity)
                .insert((NpcRoute::to_cell(target), MovementTarget::new(target)));
        }
        idle.reset_dwell(DEFAULT_NPC_IDLE_DWELL_TICKS);
    }
}

fn idle_state(
    position: CellCoord,
    assigned_house: Option<(Entity, BuildingFootprint, usize, usize)>,
    size: GridSize,
) -> AiIdleRoam {
    let Some((house, footprint, capacity, slot)) = assigned_house else {
        return AiIdleRoam::new(position, DEFAULT_NPC_IDLE_DWELL_TICKS);
    };

    let candidate_count = house_idle_roam_candidates(footprint, size).len();
    let next_offset_index = slot.saturating_mul(candidate_count) / capacity;
    AiIdleRoam::around_house(
        footprint.origin(),
        house,
        slot,
        DEFAULT_NPC_IDLE_DWELL_TICKS,
        next_offset_index,
    )
}

pub fn system_gather_resource(
    mut commands: Commands,
    mut npcs: Query<
        (
            Entity,
            &NpcPosition,
            &mut NpcInventory,
            &mut AiGatherResource,
            Option<&mut NpcSkills>,
            Option<&AiSearchForFood>,
            Option<&AiKeepEnoughFoodInInventory>,
        ),
        With<Npc>,
    >,
    mut resource_nodes: Query<(&TilePosition, &mut ResourceNode)>,
) {
    for (entity, position, mut inventory, mut gather, skills, search, keep_food) in &mut npcs {
        let target = gather.target();
        let Ok((target_position, mut resource_node)) = resource_nodes.get_mut(target) else {
            commands.entity(entity).remove::<AiGatherResource>();
            continue;
        };

        if position.coord != target_position.coord
            && !is_cardinally_adjacent(position.coord, target_position.coord)
            || resource_node.quantity == 0
        {
            commands.entity(entity).remove::<AiGatherResource>();
            continue;
        }

        gather.advance_tick();
        if gather.progress_ticks() < RESOURCE_GATHER_TICKS_PER_UNIT {
            continue;
        }

        let kind = resource_node.kind;
        if !inventory.add(kind, 1) {
            commands.entity(entity).remove::<AiGatherResource>();
            continue;
        }
        if let Some(mut skills) = skills {
            skills.add_xp(SkillKind::for_gathered_resource(kind), 1);
        }

        resource_node.quantity = resource_node.quantity.saturating_sub(1);
        if resource_node.quantity == 0 {
            commands.entity(target).remove::<ResourceNode>();
        }
        if kind == ResourceKind::Food
            && search.is_some()
            && keep_food.is_some_and(|goal| !should_continue_food_refill(&inventory, goal))
        {
            commands.entity(entity).remove::<AiSearchForFood>();
        }
        commands.entity(entity).remove::<AiGatherResource>();
    }
}

fn should_start_food_refill(
    inventory: &NpcInventory,
    keep_food: &AiKeepEnoughFoodInInventory,
) -> bool {
    let carried_food = inventory.contents().get(ResourceKind::Food);
    carried_food <= keep_food.start_threshold()
        && carried_food < keep_food.target()
        && inventory.free_size() > 0
}

fn should_continue_food_refill(
    inventory: &NpcInventory,
    keep_food: &AiKeepEnoughFoodInInventory,
) -> bool {
    inventory.contents().get(ResourceKind::Food) < keep_food.target() && inventory.free_size() > 0
}

fn should_interrupt_for_food(
    inventory: &NpcInventory,
    keep_food: Option<&AiKeepEnoughFoodInInventory>,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> bool {
    let Some(keep_food) = keep_food else {
        return false;
    };

    should_start_food_refill(inventory, keep_food)
        && resource_kind_available(ResourceKind::Food, resource_nodes)
}

fn construction_work_target(
    origin: CellCoord,
    inventory: &NpcInventory,
    tasks: &Query<&ProgressBuildingConstruction>,
    blueprints: &Query<(Entity, &BuildingBlueprint, &ConstructionProgress)>,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> Option<Entity> {
    tasks
        .iter()
        .filter_map(|task| {
            let Ok((entity, blueprint, progress)) = blueprints.get(task.blueprint()) else {
                return None;
            };
            let cost = blueprint.kind.definition().construction_cost();
            if progress.is_complete(cost)
                || !construction_work_is_actionable(inventory, progress, cost, resource_nodes)
            {
                return None;
            }

            let footprint_origin = blueprint.footprint.origin();
            Some((
                entity,
                manhattan_distance(origin, footprint_origin),
                footprint_origin.y(),
                footprint_origin.x(),
                entity.to_bits(),
            ))
        })
        .min_by_key(|(_, distance, y, x, bits)| (*distance, *y, *x, *bits))
        .map(|(entity, _, _, _, _)| entity)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlotWorkTarget {
    SeedField(Entity),
    HarvestField(Entity),
    SeedTreePlot(Entity),
    CutTreePlot(Entity),
}

fn plot_work_target(
    origin: CellCoord,
    is_farmer: bool,
    is_forester: bool,
    seed_tasks: &Query<&SeedField>,
    harvest_tasks: &Query<&HarvestField>,
    tree_seed_tasks: &Query<&SeedTreePlot>,
    tree_cut_tasks: &Query<&CutTreePlot>,
    claimed_fields: &std::collections::HashSet<Entity>,
    claimed_tree_plots: &std::collections::HashSet<Entity>,
    fields: &Query<(Entity, &Building, &FieldOwner, &FieldCrop)>,
    farms: &Query<(&Building, &FarmInventory)>,
    tree_plots: &Query<(Entity, &Building, &TreePlotOwner, &TreePlotGrowth)>,
    lodges: &Query<(&Building, &ForesterLodgeInventory)>,
) -> Option<PlotWorkTarget> {
    let seed_candidates = seed_tasks.iter().filter_map(|task| {
        if !is_farmer {
            return None;
        }
        let field = task.field();
        if claimed_fields.contains(&field) {
            return None;
        }
        let coord = field_seeding_is_actionable(field, fields, farms)?;
        Some((
            PlotWorkTarget::SeedField(field),
            manhattan_distance(origin, coord),
            coord.y(),
            coord.x(),
            field.to_bits(),
            1u8,
        ))
    });

    let harvest_candidates = harvest_tasks.iter().filter_map(|task| {
        if !is_farmer {
            return None;
        }
        let field = task.field();
        if claimed_fields.contains(&field) {
            return None;
        }
        let coord = field_harvest_is_actionable(field, fields, farms)?;
        Some((
            PlotWorkTarget::HarvestField(field),
            manhattan_distance(origin, coord),
            coord.y(),
            coord.x(),
            field.to_bits(),
            0u8,
        ))
    });

    let tree_seed_candidates = tree_seed_tasks.iter().filter_map(|task| {
        if !is_forester {
            return None;
        }
        let plot = task.tree_plot();
        if claimed_tree_plots.contains(&plot) {
            return None;
        }
        let coord = tree_plot_seeding_is_actionable(plot, tree_plots, lodges)?;
        Some((
            PlotWorkTarget::SeedTreePlot(plot),
            manhattan_distance(origin, coord),
            coord.y(),
            coord.x(),
            plot.to_bits(),
            3u8,
        ))
    });

    let tree_cut_candidates = tree_cut_tasks.iter().filter_map(|task| {
        if !is_forester {
            return None;
        }
        let plot = task.tree_plot();
        if claimed_tree_plots.contains(&plot) {
            return None;
        }
        let coord = tree_plot_cutting_is_actionable(plot, tree_plots, lodges)?;
        Some((
            PlotWorkTarget::CutTreePlot(plot),
            manhattan_distance(origin, coord),
            coord.y(),
            coord.x(),
            plot.to_bits(),
            2u8,
        ))
    });

    seed_candidates
        .chain(harvest_candidates)
        .chain(tree_seed_candidates)
        .chain(tree_cut_candidates)
        .min_by_key(|(_, distance, y, x, bits, task_index)| (*distance, *y, *x, *bits, *task_index))
        .map(|(target, _, _, _, _, _)| target)
}

fn route_to_farming_cell(
    commands: &mut Commands,
    entity: Entity,
    position: &NpcPosition,
    movement_target: Option<&MovementTarget>,
    coord: CellCoord,
) {
    if position.coord == coord {
        if movement_target.is_some() {
            commands.entity(entity).remove::<MovementTarget>();
        }
        return;
    }

    if movement_target.map(|movement_target| movement_target.coord) != Some(coord) {
        commands
            .entity(entity)
            .insert((NpcRoute::to_cell(coord), MovementTarget::new(coord)));
    }
}

fn construction_work_is_actionable(
    inventory: &NpcInventory,
    progress: &ConstructionProgress,
    cost: ResourceAmounts,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> bool {
    has_depositable_construction_resources(inventory, progress, cost)
        || (inventory.free_size() > 0
            && ResourceKind::ALL.into_iter().any(|kind| {
                progress.remaining(cost, kind) > 0 && resource_kind_available(kind, resource_nodes)
            }))
}

fn has_depositable_construction_resources(
    inventory: &NpcInventory,
    progress: &ConstructionProgress,
    cost: ResourceAmounts,
) -> bool {
    ResourceKind::ALL
        .into_iter()
        .any(|kind| inventory.contents().get(kind) > 0 && progress.remaining(cost, kind) > 0)
}

fn should_route_to_construction_deposit(
    inventory: &NpcInventory,
    construction: &AiConstructBuilding,
    progress: &ConstructionProgress,
    cost: ResourceAmounts,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> bool {
    if !has_depositable_construction_resources(inventory, progress, cost) {
        return false;
    }

    let Some(kind) = construction.target_kind() else {
        return true;
    };

    let remaining = progress.remaining(cost, kind);
    let carried = inventory.contents().get(kind);
    if remaining == 0 || carried == 0 {
        return true;
    }

    let batch_target = remaining.min(CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE);
    carried >= batch_target || !resource_kind_available(kind, resource_nodes)
}

fn construction_resource_target_kind(
    origin: CellCoord,
    progress: &ConstructionProgress,
    cost: ResourceAmounts,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> Option<ResourceKind> {
    ResourceKind::ALL
        .into_iter()
        .filter(|kind| progress.remaining(cost, *kind) > 0)
        .filter_map(|kind| {
            let (entity, coord) = nearest_resource_node_of_kind(origin, kind, resource_nodes)?;
            Some((
                kind,
                manhattan_distance(origin, coord),
                coord.y(),
                coord.x(),
                entity.to_bits(),
                kind as usize,
            ))
        })
        .min_by_key(|(_, distance, y, x, bits, kind_index)| (*distance, *y, *x, *bits, *kind_index))
        .map(|(kind, _, _, _, _, _)| kind)
}

fn resource_kind_available(
    kind: ResourceKind,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> bool {
    resource_nodes
        .iter()
        .any(|(_, _, node)| node.kind == kind && node.quantity > 0)
}

fn route_to_building_footprint(
    commands: &mut Commands,
    entity: Entity,
    position: &NpcPosition,
    movement_target: Option<&MovementTarget>,
    footprint: BuildingFootprint,
) {
    if footprint.contains(position.coord)
        || footprint
            .iter_coords()
            .any(|coord| is_cardinally_adjacent(position.coord, coord))
    {
        if movement_target.is_some() {
            commands.entity(entity).remove::<MovementTarget>();
        }
        return;
    }

    let goals = footprint
        .iter_coords()
        .flat_map(cardinal_interaction_cells)
        .filter(|coord| !footprint.contains(*coord))
        .collect::<Vec<_>>();
    commands.entity(entity).insert((
        NpcRoute::new(goals),
        MovementTarget::new(footprint.origin()),
    ));
}

fn cardinal_interaction_cells(coord: CellCoord) -> Vec<CellCoord> {
    let mut cells = Vec::with_capacity(4);
    if let Some(y) = coord.y().checked_sub(1) {
        cells.push(CellCoord::new(coord.x(), y));
    }
    if let Some(x) = coord.x().checked_sub(1) {
        cells.push(CellCoord::new(x, coord.y()));
    }
    if let Some(x) = coord.x().checked_add(1) {
        cells.push(CellCoord::new(x, coord.y()));
    }
    if let Some(y) = coord.y().checked_add(1) {
        cells.push(CellCoord::new(coord.x(), y));
    }
    cells
}

fn is_cardinally_adjacent(a: CellCoord, b: CellCoord) -> bool {
    a.x().abs_diff(b.x()).saturating_add(a.y().abs_diff(b.y())) == 1
}

fn building_interaction_reached(
    kind: crate::buildings::BuildingKind,
    footprint: BuildingFootprint,
    position: CellCoord,
) -> bool {
    if footprint.contains(position) {
        true
    } else if matches!(
        kind,
        crate::buildings::BuildingKind::Field | crate::buildings::BuildingKind::TreePlot
    ) {
        false
    } else {
        footprint
            .iter_coords()
            .any(|coord| is_cardinally_adjacent(position, coord))
    }
}

fn idle_roam_target(
    origin: CellCoord,
    current: CellCoord,
    size: GridSize,
    start_offset_index: usize,
) -> Option<(CellCoord, usize)> {
    for step in 0..IDLE_ROAM_OFFSETS.len() {
        let index = (start_offset_index + step) % IDLE_ROAM_OFFSETS.len();
        let offset = IDLE_ROAM_OFFSETS[index];
        let Some(target) = offset_coord(origin, offset) else {
            continue;
        };
        if target == current || !size.contains(target) {
            continue;
        }

        return Some((target, (index + 1) % IDLE_ROAM_OFFSETS.len()));
    }

    None
}

fn house_idle_roam_target(
    footprint: BuildingFootprint,
    current: CellCoord,
    size: GridSize,
    start_offset_index: usize,
    navigation: &NavigationSnapshot,
) -> Option<(CellCoord, usize)> {
    let candidates = house_idle_roam_candidates(footprint, size);
    if candidates.is_empty() {
        return None;
    }
    let distances = navigation.distances_from(current)?;

    for step in 0..candidates.len() {
        let index = (start_offset_index + step) % candidates.len();
        let target = candidates[index];
        if target != current && distances.is_reachable(target) {
            return Some((target, (index + 1) % candidates.len()));
        }
    }

    None
}

fn house_idle_roam_candidates(footprint: BuildingFootprint, size: GridSize) -> Vec<CellCoord> {
    let Ok(width) = i32::try_from(footprint.width()) else {
        return Vec::new();
    };
    let Ok(height) = i32::try_from(footprint.height()) else {
        return Vec::new();
    };
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let left = footprint.origin().x();
    let top = footprint.origin().y();
    let Some(right) = left.checked_add(width - 1) else {
        return Vec::new();
    };
    let Some(bottom) = top.checked_add(height - 1) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();

    for distance in 1..=DEFAULT_NPC_IDLE_ROAM_RADIUS as i32 {
        let Some(ring_top) = top.checked_sub(distance) else {
            continue;
        };
        let Some(ring_right) = right.checked_add(distance) else {
            continue;
        };
        let Some(ring_bottom) = bottom.checked_add(distance) else {
            continue;
        };
        let Some(ring_left) = left.checked_sub(distance) else {
            continue;
        };

        push_horizontal(&mut candidates, size, left, right, ring_top, false);
        for diagonal in 1..distance {
            push_candidate(
                &mut candidates,
                size,
                CellCoord::new(right + diagonal, top - (distance - diagonal)),
            );
        }
        push_vertical(&mut candidates, size, top, bottom, ring_right, false);
        for diagonal in 1..distance {
            push_candidate(
                &mut candidates,
                size,
                CellCoord::new(right + (distance - diagonal), bottom + diagonal),
            );
        }
        push_horizontal(&mut candidates, size, left, right, ring_bottom, true);
        for diagonal in 1..distance {
            push_candidate(
                &mut candidates,
                size,
                CellCoord::new(left - diagonal, bottom + (distance - diagonal)),
            );
        }
        push_vertical(&mut candidates, size, top, bottom, ring_left, true);
        for diagonal in 1..distance {
            push_candidate(
                &mut candidates,
                size,
                CellCoord::new(left - (distance - diagonal), top - diagonal),
            );
        }
    }

    candidates
}

fn push_horizontal(
    candidates: &mut Vec<CellCoord>,
    size: GridSize,
    left: i32,
    right: i32,
    y: i32,
    reverse: bool,
) {
    if reverse {
        for x in (left..=right).rev() {
            push_candidate(candidates, size, CellCoord::new(x, y));
        }
    } else {
        for x in left..=right {
            push_candidate(candidates, size, CellCoord::new(x, y));
        }
    }
}

fn push_vertical(
    candidates: &mut Vec<CellCoord>,
    size: GridSize,
    top: i32,
    bottom: i32,
    x: i32,
    reverse: bool,
) {
    if reverse {
        for y in (top..=bottom).rev() {
            push_candidate(candidates, size, CellCoord::new(x, y));
        }
    } else {
        for y in top..=bottom {
            push_candidate(candidates, size, CellCoord::new(x, y));
        }
    }
}

fn push_candidate(candidates: &mut Vec<CellCoord>, size: GridSize, candidate: CellCoord) {
    if size.contains(candidate) {
        candidates.push(candidate);
    }
}

fn offset_coord(origin: CellCoord, offset: (i32, i32)) -> Option<CellCoord> {
    Some(CellCoord::new(
        origin.x().checked_add(offset.0)?,
        origin.y().checked_add(offset.1)?,
    ))
}

fn nearest_food_resource(
    origin: CellCoord,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> Option<(Entity, CellCoord)> {
    nearest_resource_node_of_kind(origin, ResourceKind::Food, resource_nodes)
}

fn nearest_resource_node_of_kind(
    origin: CellCoord,
    kind: ResourceKind,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> Option<(Entity, CellCoord)> {
    resource_nodes
        .iter()
        .filter(|(_, _, node)| node.kind == kind && node.quantity > 0)
        .min_by_key(|(entity, position, _)| {
            (
                manhattan_distance(origin, position.coord),
                position.coord.y(),
                position.coord.x(),
                entity.to_bits(),
            )
        })
        .map(|(entity, position, _)| (entity, position.coord))
}

fn manhattan_distance(a: CellCoord, b: CellCoord) -> u32 {
    a.x().abs_diff(b.x()).saturating_add(a.y().abs_diff(b.y()))
}
