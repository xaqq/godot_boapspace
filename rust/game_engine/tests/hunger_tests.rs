use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::npcs::{
    update_npc_hunger, HungerState, Npc, NpcHunger, NpcInventory, SimulationTickDuration,
    SECONDS_PER_DAY,
};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use std::time::Duration;

#[test]
fn test_npc_consumes_food_when_satiation_runs_out() {
    let mut world = world_with_tick(Duration::from_secs(SECONDS_PER_DAY));
    let npc = spawn_npc(
        &mut world,
        NpcHunger::fed(),
        NpcInventory::new(ResourceAmounts::new(0, 0, 1, 0)),
    );

    world
        .run_system_once(update_npc_hunger)
        .expect("hunger system should run");

    assert_eq!(hunger(&world, npc).state(), HungerState::Fed);
    assert_eq!(inventory(&world, npc).contents().get(ResourceKind::Food), 0);
}

#[test]
fn test_hungry_npc_recovers_when_food_is_available() {
    let mut world = world_with_tick(Duration::from_secs(10 * 60));
    let npc = spawn_npc(
        &mut world,
        NpcHunger::new(Duration::ZERO, Duration::from_secs(60)),
        NpcInventory::new(ResourceAmounts::new(0, 0, 1, 0)),
    );

    world
        .run_system_once(update_npc_hunger)
        .expect("hunger system should run");

    let hunger = hunger(&world, npc);
    assert_eq!(hunger.state(), HungerState::Fed);
    assert_eq!(hunger.hunger_duration(), Duration::ZERO);
    assert_eq!(inventory(&world, npc).contents().get(ResourceKind::Food), 0);
}

fn world_with_tick(duration: Duration) -> World {
    let mut world = World::new();
    world.insert_resource(SimulationTickDuration::new(duration));
    world
}

fn spawn_npc(world: &mut World, hunger: NpcHunger, inventory: NpcInventory) -> Entity {
    world.spawn((Npc, hunger, inventory)).id()
}

fn hunger(world: &World, entity: Entity) -> NpcHunger {
    *world
        .get::<NpcHunger>(entity)
        .expect("NPC should have hunger")
}

fn inventory(world: &World, entity: Entity) -> NpcInventory {
    *world
        .get::<NpcInventory>(entity)
        .expect("NPC should have inventory")
}
