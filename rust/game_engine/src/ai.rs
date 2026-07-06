pub use crate::components::{AiGatherResource, AiKeepEnoughFoodInInventory, AiSearchForFood};

use crate::components::{
    MovementTarget, Npc, NpcInventory, NpcPosition, ResourceNode, TilePosition,
};
use crate::grid::CellCoord;
use crate::resources::ResourceKind;
use bevy_ecs::prelude::*;

pub const DEFAULT_NPC_FOOD_INVENTORY_TARGET: u32 = 20;
pub const RESOURCE_GATHER_TICKS_PER_UNIT: u32 = 60;

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
        if inventory.contents().get(ResourceKind::Food) < goal.target() && search.is_none() {
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

        inventory.add(ResourceKind::Food, 1);
        resource_node.quantity = resource_node.quantity.saturating_sub(1);
        if resource_node.quantity == 0 {
            commands.entity(target).remove::<ResourceNode>();
        }
        commands.entity(entity).remove::<AiGatherResource>();
    }
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
