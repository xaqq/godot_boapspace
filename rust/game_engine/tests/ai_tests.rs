use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::ai::{
    system_gather_resource, system_keep_enough_food_in_inventory, system_search_for_food,
    AiGatherResource, AiKeepEnoughFoodInInventory, AiSearchForFood,
    DEFAULT_NPC_FOOD_INVENTORY_TARGET, RESOURCE_GATHER_TICKS_PER_UNIT,
};
use game_engine::components::{MovementTarget, NpcInventory, ResourceNode, Tile, TilePosition};
use game_engine::grid::CellCoord;
use game_engine::npcs::{Npc, NpcPosition};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::simulation::GameSimulation;

#[test]
fn test_initial_npc_has_keep_food_goal() {
    let simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();

    let target = simulation
        .with_surface_world(surface, |world| {
            let mut query = world.try_query::<(&AiKeepEnoughFoodInInventory, &Npc)>()?;
            query.iter(world).next().map(|(goal, _)| goal.target())
        })
        .expect("default NPC should have keep-food goal");

    assert_eq!(target, DEFAULT_NPC_FOOD_INVENTORY_TARGET);
}

#[test]
fn test_keep_enough_food_adds_search_when_below_target() {
    let mut world = World::new();
    let npc = world
        .spawn((
            Npc,
            NpcInventory::empty(),
            AiKeepEnoughFoodInInventory::new(DEFAULT_NPC_FOOD_INVENTORY_TARGET),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_some());
}

#[test]
fn test_keep_enough_food_does_not_add_search_when_at_target() {
    let mut world = World::new();
    let npc = world
        .spawn((
            Npc,
            NpcInventory::new(ResourceAmounts::new(
                0,
                0,
                DEFAULT_NPC_FOOD_INVENTORY_TARGET,
                0,
            )),
            AiKeepEnoughFoodInInventory::new(DEFAULT_NPC_FOOD_INVENTORY_TARGET),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_none());
}

#[test]
fn test_search_for_food_sets_target_to_nearest_food_node() {
    let mut world = World::new();
    let npc = spawn_searching_npc(&mut world, CellCoord::new(1, 1));
    spawn_resource_node(&mut world, CellCoord::new(1, 2), ResourceKind::Wood, 10);
    spawn_resource_node(&mut world, CellCoord::new(1, 3), ResourceKind::Food, 10);
    spawn_resource_node(&mut world, CellCoord::new(3, 1), ResourceKind::Food, 10);

    run_search_for_food(&mut world);

    assert_eq!(
        world
            .get::<MovementTarget>(npc)
            .expect("search should set movement target")
            .coord,
        CellCoord::new(3, 1)
    );
    assert!(world.get::<AiSearchForFood>(npc).is_some());
}

#[test]
fn test_search_for_food_retries_when_no_food_exists() {
    let mut world = World::new();
    let npc = spawn_searching_npc(&mut world, CellCoord::new(1, 1));
    spawn_resource_node(&mut world, CellCoord::new(1, 2), ResourceKind::Wood, 10);

    run_search_for_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_some());
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_search_for_food_starts_gathering_at_resource_tile() {
    let mut world = World::new();
    let npc = spawn_searching_npc(&mut world, CellCoord::new(2, 1));
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Food, 10);

    run_search_for_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_none());
    assert_eq!(
        world
            .get::<AiGatherResource>(npc)
            .expect("search should start gathering")
            .target(),
        resource
    );
}

#[test]
fn test_search_for_food_preserves_existing_gather_progress_for_same_target() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Food, 10);
    let mut gather = AiGatherResource::new(resource);
    gather.advance_tick();
    gather.advance_tick();
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(2, 1)),
            AiSearchForFood,
            gather,
        ))
        .id();

    run_search_for_food(&mut world);

    let gather = *world
        .get::<AiGatherResource>(npc)
        .expect("gather should be preserved");
    assert_eq!(gather.target(), resource);
    assert_eq!(gather.progress_ticks(), 2);
    assert!(world.get::<AiSearchForFood>(npc).is_none());
}

#[test]
fn test_gather_resource_completes_after_sixty_valid_ticks() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Food, 2);
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(2, 1), resource);

    for _ in 0..(RESOURCE_GATHER_TICKS_PER_UNIT - 1) {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_food(&world, npc), 0);
    assert_eq!(
        world
            .get::<AiGatherResource>(npc)
            .expect("gather should still be active")
            .progress_ticks(),
        RESOURCE_GATHER_TICKS_PER_UNIT - 1
    );
    assert_eq!(resource_quantity(&world, resource), Some(2));

    run_gather_resource(&mut world);

    assert_eq!(npc_food(&world, npc), 1);
    assert!(world.get::<AiGatherResource>(npc).is_none());
    assert_eq!(resource_quantity(&world, resource), Some(1));
}

#[test]
fn test_gather_resource_removes_depleted_resource_node_not_tile() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Food, 1);
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(2, 1), resource);

    for _ in 0..RESOURCE_GATHER_TICKS_PER_UNIT {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_food(&world, npc), 1);
    assert!(world.get::<ResourceNode>(resource).is_none());
    assert!(world.get::<TilePosition>(resource).is_some());
    assert!(world.get::<Tile>(resource).is_some());
}

#[test]
fn test_gather_resource_removes_invalid_target_without_awarding_food() {
    let mut world = World::new();
    let stale_target = world.spawn_empty().id();
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(2, 1), stale_target);

    run_gather_resource(&mut world);

    assert_eq!(npc_food(&world, npc), 0);
    assert!(world.get::<AiGatherResource>(npc).is_none());
}

#[test]
fn test_gather_resource_removes_moved_away_gather_without_awarding_food() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Food, 1);
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(3, 1), resource);

    run_gather_resource(&mut world);

    assert_eq!(npc_food(&world, npc), 0);
    assert!(world.get::<AiGatherResource>(npc).is_none());
    assert_eq!(resource_quantity(&world, resource), Some(1));
}

fn spawn_searching_npc(world: &mut World, coord: CellCoord) -> Entity {
    world
        .spawn((Npc, NpcPosition::new(coord), AiSearchForFood))
        .id()
}

fn spawn_gathering_npc(world: &mut World, coord: CellCoord, target: Entity) -> Entity {
    world
        .spawn((
            Npc,
            NpcPosition::new(coord),
            NpcInventory::empty(),
            AiGatherResource::new(target),
        ))
        .id()
}

fn spawn_resource_node(
    world: &mut World,
    coord: CellCoord,
    kind: ResourceKind,
    quantity: u32,
) -> Entity {
    world
        .spawn((
            Tile,
            TilePosition { coord },
            ResourceNode { kind, quantity },
        ))
        .id()
}

fn run_keep_enough_food(world: &mut World) {
    world
        .run_system_once(system_keep_enough_food_in_inventory)
        .expect("keep-enough-food system should run");
}

fn run_search_for_food(world: &mut World) {
    world
        .run_system_once(system_search_for_food)
        .expect("search-for-food system should run");
}

fn run_gather_resource(world: &mut World) {
    world
        .run_system_once(system_gather_resource)
        .expect("gather-resource system should run");
}

fn npc_food(world: &World, npc: Entity) -> u32 {
    world
        .get::<NpcInventory>(npc)
        .expect("NPC should have inventory")
        .contents()
        .get(ResourceKind::Food)
}

fn resource_quantity(world: &World, entity: Entity) -> Option<u32> {
    world
        .get::<ResourceNode>(entity)
        .map(|resource| resource.quantity)
}
