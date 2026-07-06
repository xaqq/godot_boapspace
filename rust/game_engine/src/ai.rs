pub use crate::components::{
    AiGatherResource, AiIdleRoam, AiKeepEnoughFoodInInventory, AiSearchForFood,
};

use crate::components::{
    MovementTarget, Npc, NpcInventory, NpcPosition, ResourceNode, TilePosition,
};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::resources::ResourceKind;
use bevy_ecs::prelude::*;

pub const DEFAULT_NPC_FOOD_INVENTORY_TARGET: u32 = 20;
pub const DEFAULT_NPC_IDLE_ROAM_RADIUS: u32 = 3;
pub const DEFAULT_NPC_IDLE_DWELL_TICKS: u32 = 180;
pub const RESOURCE_GATHER_TICKS_PER_UNIT: u32 = 60;

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
) {
    for (entity, inventory, goal, search) in &npcs {
        if inventory.contents().get(ResourceKind::Food) < goal.target()
            && inventory.free_size() > 0
            && search.is_none()
        {
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
            Option<&AiGatherResource>,
            Option<&MovementTarget>,
        ),
        (With<Npc>, With<AiSearchForFood>),
    >,
    resource_nodes: Query<(Entity, &TilePosition, &ResourceNode)>,
) {
    for (entity, position, gather, movement_target) in &npcs {
        let Some((resource_entity, resource_coord)) =
            nearest_food_resource(position.coord, &resource_nodes)
        else {
            continue;
        };

        if position.coord == resource_coord {
            commands.entity(entity).remove::<AiSearchForFood>();
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
            Option<&mut AiIdleRoam>,
        ),
        With<Npc>,
    >,
) {
    let size = grid.size();
    for (entity, position, inventory, keep_food, movement_target, search, gather, idle) in &mut npcs
    {
        if search.is_some() || gather.is_some() {
            commands.entity(entity).remove::<AiIdleRoam>();
            continue;
        }

        if needs_food(inventory, keep_food) {
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
        ),
        With<Npc>,
    >,
    mut resource_nodes: Query<(&TilePosition, &mut ResourceNode)>,
) {
    for (entity, position, mut inventory, mut gather) in &mut npcs {
        let target = gather.target();
        let Ok((target_position, mut resource_node)) = resource_nodes.get_mut(target) else {
            commands.entity(entity).remove::<AiGatherResource>();
            continue;
        };

        if target_position.coord != position.coord
            || resource_node.kind != ResourceKind::Food
            || resource_node.quantity == 0
        {
            commands.entity(entity).remove::<AiGatherResource>();
            continue;
        }

        gather.advance_tick();
        if gather.progress_ticks() < RESOURCE_GATHER_TICKS_PER_UNIT {
            continue;
        }

        if !inventory.add(ResourceKind::Food, 1) {
            commands.entity(entity).remove::<AiGatherResource>();
            continue;
        }

        resource_node.quantity = resource_node.quantity.saturating_sub(1);
        if resource_node.quantity == 0 {
            commands.entity(target).remove::<ResourceNode>();
        }
        commands.entity(entity).remove::<AiGatherResource>();
    }
}

fn needs_food(
    inventory: Option<&NpcInventory>,
    keep_food: Option<&AiKeepEnoughFoodInInventory>,
) -> bool {
    let Some(inventory) = inventory else {
        return false;
    };
    let Some(keep_food) = keep_food else {
        return false;
    };

    inventory.contents().get(ResourceKind::Food) < keep_food.target() && inventory.free_size() > 0
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
    resource_nodes
        .iter()
        .filter(|(_, _, node)| node.kind == ResourceKind::Food && node.quantity > 0)
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
