pub use crate::components::{
    AiConstructBuilding, AiGatherResource, AiIdleRoam, AiKeepEnoughFoodInInventory, AiSearchForFood,
};

use crate::buildings::{BuildingBlueprint, BuildingFootprint, ConstructionProgress};
use crate::components::{
    MovementTarget, Npc, NpcInventory, NpcPosition, ResourceNode, TilePosition,
};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::resources::{ResourceAmounts, ResourceKind};
use crate::tasks::ProgressBuildingConstruction;
use bevy_ecs::prelude::*;

pub const DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD: u32 = 5;
pub const DEFAULT_NPC_FOOD_INVENTORY_TARGET: u32 = 20;
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

        if position.coord == resource_coord {
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
            commands
                .entity(entity)
                .insert(MovementTarget::new(resource_coord));
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
            Option<&AiSearchForFood>,
            Option<&AiGatherResource>,
            Option<&AiConstructBuilding>,
        ),
        With<Npc>,
    >,
    tasks: Query<&ProgressBuildingConstruction>,
    blueprints: Query<(Entity, &BuildingBlueprint, &ConstructionProgress)>,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    for (entity, position, inventory, keep_food, search, gather, construction) in &npcs {
        if search.is_some()
            || gather.is_some()
            || construction.is_some()
            || should_interrupt_for_food(inventory, keep_food, &resource_nodes)
        {
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
            Option<&MovementTarget>,
        ),
        With<Npc>,
    >,
    blueprints: Query<(&BuildingBlueprint, &ConstructionProgress)>,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    for (entity, position, inventory, mut construction, search, gather, movement_target) in
        &mut npcs
    {
        if search.is_some() || gather.is_some() {
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

        if position.coord == resource_coord {
            if movement_target.is_some() {
                commands.entity(entity).remove::<MovementTarget>();
            }
            commands
                .entity(entity)
                .insert(AiGatherResource::new(resource_entity));
        } else if movement_target.map(|target| target.coord) != Some(resource_coord) {
            commands
                .entity(entity)
                .insert(MovementTarget::new(resource_coord));
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
        ),
        With<Npc>,
    >,
    mut blueprints: Query<(&BuildingBlueprint, &mut ConstructionProgress)>,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    for (entity, position, mut inventory, mut construction, search, gather) in &mut npcs {
        if search.is_some() || gather.is_some() {
            continue;
        }

        let Ok((blueprint, mut progress)) = blueprints.get_mut(construction.blueprint()) else {
            commands.entity(entity).remove::<AiConstructBuilding>();
            continue;
        };
        if !blueprint.footprint.contains(position.coord) {
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
    mut npcs: Query<
        (
            Entity,
            &NpcPosition,
            Option<&NpcInventory>,
            Option<&AiKeepEnoughFoodInInventory>,
            Option<&MovementTarget>,
            Option<&AiSearchForFood>,
            Option<&AiGatherResource>,
            Option<&AiConstructBuilding>,
            Option<&mut AiIdleRoam>,
        ),
        With<Npc>,
    >,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    let size = grid.size();
    for (
        entity,
        position,
        inventory,
        keep_food,
        movement_target,
        search,
        gather,
        construction,
        idle,
    ) in &mut npcs
    {
        if search.is_some() || gather.is_some() || construction.is_some() {
            commands.entity(entity).remove::<AiIdleRoam>();
            continue;
        }

        if should_interrupt_for_food_opt(inventory, keep_food, &resource_nodes) {
            if idle.is_some() {
                commands.entity(entity).remove::<AiIdleRoam>();
                commands.entity(entity).remove::<MovementTarget>();
            }
            continue;
        }

        if movement_target.is_some() {
            continue;
        }

        let Some(mut idle) = idle else {
            commands.entity(entity).insert(AiIdleRoam::new(
                position.coord,
                DEFAULT_NPC_IDLE_DWELL_TICKS,
            ));
            continue;
        };

        if idle.dwell_ticks_remaining() > 0 {
            idle.advance_dwell();
            if idle.dwell_ticks_remaining() > 0 {
                continue;
            }
        }

        if let Some((target, next_offset_index)) = idle_roam_target(
            idle.origin(),
            position.coord,
            size,
            idle.next_offset_index(),
        ) {
            idle.set_next_offset_index(next_offset_index);
            commands.entity(entity).insert(MovementTarget::new(target));
        }
        idle.reset_dwell(DEFAULT_NPC_IDLE_DWELL_TICKS);
    }
}

pub fn system_gather_resource(
    mut commands: Commands,
    mut npcs: Query<
        (
            Entity,
            &NpcPosition,
            &mut NpcInventory,
            &mut AiGatherResource,
            Option<&AiSearchForFood>,
            Option<&AiKeepEnoughFoodInInventory>,
        ),
        With<Npc>,
    >,
    mut resource_nodes: Query<(&TilePosition, &mut ResourceNode)>,
) {
    for (entity, position, mut inventory, mut gather, search, keep_food) in &mut npcs {
        let target = gather.target();
        let Ok((target_position, mut resource_node)) = resource_nodes.get_mut(target) else {
            commands.entity(entity).remove::<AiGatherResource>();
            continue;
        };

        if target_position.coord != position.coord || resource_node.quantity == 0 {
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

fn should_interrupt_for_food_opt(
    inventory: Option<&NpcInventory>,
    keep_food: Option<&AiKeepEnoughFoodInInventory>,
    resource_nodes: &Query<(Entity, &TilePosition, &ResourceNode)>,
) -> bool {
    let Some(inventory) = inventory else {
        return false;
    };

    should_interrupt_for_food(inventory, keep_food, resource_nodes)
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
    if footprint.contains(position.coord) {
        if movement_target.is_some() {
            commands.entity(entity).remove::<MovementTarget>();
        }
        return;
    }

    let target = footprint.origin();
    if movement_target.map(|movement_target| movement_target.coord) != Some(target) {
        commands.entity(entity).insert(MovementTarget::new(target));
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
