use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::components::{NPC_HUNGER_FULL_SATIATION, NPC_HUNGER_HUNGRY_THRESHOLD};
use game_engine::npcs::{update_npc_hunger, HungerState, Npc, NpcHunger, NpcInventory};
use game_engine::resources::{ResourceAmounts, ResourceKind};

#[test]
fn test_fed_npc_consumes_food_when_reaching_hungry_threshold() {
    let mut world = World::new();
    let npc = spawn_npc(
        &mut world,
        NpcHunger::new(NPC_HUNGER_HUNGRY_THRESHOLD + 1),
        NpcInventory::new(ResourceAmounts::new(0, 0, 1, 0)),
    );

    world
        .run_system_once(update_npc_hunger)
        .expect("hunger system should run");

    let hunger = npc_hunger(&world, npc);
    assert_eq!(hunger.state(), HungerState::Fed);
    assert_eq!(hunger.satiation_level(), NPC_HUNGER_FULL_SATIATION);
    assert_eq!(inventory(&world, npc).contents().get(ResourceKind::Food), 0);
}

#[test]
fn test_hungry_npc_recovers_when_food_is_available() {
    let mut world = World::new();
    let npc = spawn_npc(
        &mut world,
        NpcHunger::new(NPC_HUNGER_HUNGRY_THRESHOLD),
        NpcInventory::new(ResourceAmounts::new(0, 0, 1, 0)),
    );

    world
        .run_system_once(update_npc_hunger)
        .expect("hunger system should run");

    let hunger = npc_hunger(&world, npc);
    assert_eq!(hunger.state(), HungerState::Fed);
    assert_eq!(
        hunger.satiation_level(),
        NPC_HUNGER_FULL_SATIATION.saturating_sub(1)
    );
    assert_eq!(inventory(&world, npc).contents().get(ResourceKind::Food), 0);
}

#[test]
fn test_no_food_npc_moves_from_fed_to_hungry_to_starving() {
    let mut world = World::new();
    let npc = spawn_npc(
        &mut world,
        NpcHunger::new(NPC_HUNGER_HUNGRY_THRESHOLD + 1),
        NpcInventory::empty(),
    );

    world
        .run_system_once(update_npc_hunger)
        .expect("hunger system should run");

    let hunger = npc_hunger(&world, npc);
    assert_eq!(hunger.state(), HungerState::Hungry);
    assert_eq!(hunger.satiation_level(), NPC_HUNGER_HUNGRY_THRESHOLD);

    for _ in 0..NPC_HUNGER_HUNGRY_THRESHOLD {
        world
            .run_system_once(update_npc_hunger)
            .expect("hunger system should run");
    }

    let hunger = npc_hunger(&world, npc);
    assert_eq!(hunger.state(), HungerState::Starving);
    assert_eq!(hunger.satiation_level(), 0);
}

#[test]
fn test_zero_satiation_is_starving() {
    assert_eq!(NpcHunger::new(0).state(), HungerState::Starving);
}

fn spawn_npc(world: &mut World, hunger: NpcHunger, inventory: NpcInventory) -> Entity {
    world.spawn((Npc, hunger, inventory)).id()
}

fn npc_hunger(world: &World, entity: Entity) -> NpcHunger {
    *world
        .get::<NpcHunger>(entity)
        .expect("NPC should have hunger")
}

fn inventory(world: &World, entity: Entity) -> NpcInventory {
    *world
        .get::<NpcInventory>(entity)
        .expect("NPC should have inventory")
}
