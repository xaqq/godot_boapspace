use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::ai::{
    system_gather_resource, system_keep_enough_food_in_inventory, system_npc_idle,
    system_search_for_food, AiGatherResource, AiIdleRoam, AiKeepEnoughFoodInInventory,
    AiSearchForFood, DEFAULT_NPC_FOOD_INVENTORY_TARGET, DEFAULT_NPC_IDLE_DWELL_TICKS,
    DEFAULT_NPC_IDLE_ROAM_RADIUS, RESOURCE_GATHER_TICKS_PER_UNIT,
};
use game_engine::components::{
    MovementTarget, NpcInventory, ResourceNode, Tile, TilePosition, DEFAULT_NPC_INVENTORY_MAX_SIZE,
};
use game_engine::grid::{CellCoord, Grid};
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
fn test_keep_enough_food_does_not_search_when_inventory_is_full() {
    let mut world = World::new();
    let npc = world
        .spawn((
            Npc,
            NpcInventory::new(ResourceAmounts::new(
                DEFAULT_NPC_INVENTORY_MAX_SIZE,
                0,
                0,
                0,
            )),
            AiKeepEnoughFoodInInventory::new(DEFAULT_NPC_FOOD_INVENTORY_TARGET),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_none());
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
fn test_default_npc_enters_idle_roam_when_food_is_sufficient() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();

    simulation.tick();

    let (origin, position) = simulation
        .with_surface_world(surface, |world| {
            let mut query = world.try_query::<(&AiIdleRoam, &NpcPosition, &Npc)>()?;
            query
                .iter(world)
                .next()
                .map(|(idle, position, _)| (idle.origin(), position.coord))
        })
        .expect("default NPC should start idle roaming after one tick");

    assert_eq!(origin, position);
}

#[test]
fn test_idle_roam_records_origin_and_waits_before_moving() {
    let mut world = idle_world(8, 8);
    let npc = spawn_idle_npc(&mut world, CellCoord::new(3, 3));

    run_idle(&mut world);

    let idle = idle_roam(&world, npc).expect("idle roam should be added");
    assert_eq!(idle.origin(), CellCoord::new(3, 3));
    assert_eq!(idle.dwell_ticks_remaining(), DEFAULT_NPC_IDLE_DWELL_TICKS);
    assert_eq!(idle.next_offset_index(), 0);
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_idle_roam_does_not_set_target_before_dwell_finishes() {
    let mut world = idle_world(8, 8);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(3, 3)),
            AiIdleRoam::new(CellCoord::new(3, 3), 2),
        ))
        .id();

    run_idle(&mut world);

    let idle = idle_roam(&world, npc).expect("idle roam should remain active");
    assert_eq!(idle.dwell_ticks_remaining(), 1);
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_idle_roam_sets_deterministic_target_after_dwell() {
    let origin = CellCoord::new(3, 3);
    let mut world = idle_world(8, 8);
    let npc = world
        .spawn((Npc, NpcPosition::new(origin), AiIdleRoam::new(origin, 1)))
        .id();

    run_idle(&mut world);

    let target = world
        .get::<MovementTarget>(npc)
        .expect("idle roam should set movement target")
        .coord;
    let idle = idle_roam(&world, npc).expect("idle roam should remain active");
    assert_eq!(target, CellCoord::new(4, 3));
    assert_eq!(idle.dwell_ticks_remaining(), DEFAULT_NPC_IDLE_DWELL_TICKS);
    assert_eq!(idle.next_offset_index(), 1);
    assert!(
        manhattan_distance(origin, target) <= DEFAULT_NPC_IDLE_ROAM_RADIUS,
        "{target:?} should stay within idle roam radius"
    );
}

#[test]
fn test_idle_roam_skips_current_target_near_edge() {
    let origin = CellCoord::new(0, 0);
    let mut world = idle_world(2, 2);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 0)),
            AiIdleRoam::new(origin, 1),
        ))
        .id();

    run_idle(&mut world);

    assert_eq!(
        world
            .get::<MovementTarget>(npc)
            .expect("idle roam should choose a valid target")
            .coord,
        CellCoord::new(0, 1)
    );
}

#[test]
fn test_idle_roam_waits_again_when_no_valid_target_exists() {
    let origin = CellCoord::new(0, 0);
    let mut world = idle_world(1, 1);
    let npc = world
        .spawn((Npc, NpcPosition::new(origin), AiIdleRoam::new(origin, 1)))
        .id();

    run_idle(&mut world);

    let idle = idle_roam(&world, npc).expect("idle roam should remain active");
    assert!(world.get::<MovementTarget>(npc).is_none());
    assert_eq!(idle.dwell_ticks_remaining(), DEFAULT_NPC_IDLE_DWELL_TICKS);
    assert_eq!(idle.next_offset_index(), 0);
}

#[test]
fn test_idle_roam_does_not_overwrite_existing_movement_target() {
    let origin = CellCoord::new(3, 3);
    let existing_target = CellCoord::new(7, 7);
    let mut world = idle_world(8, 8);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(origin),
            AiIdleRoam::new(origin, 0),
            MovementTarget::new(existing_target),
        ))
        .id();

    run_idle(&mut world);

    assert_eq!(
        world
            .get::<MovementTarget>(npc)
            .expect("movement target should remain")
            .coord,
        existing_target
    );
    assert_eq!(
        idle_roam(&world, npc)
            .expect("idle roam state should remain while idle movement is in progress")
            .origin(),
        origin
    );
}

#[test]
fn test_idle_roam_is_suppressed_by_search_and_gather() {
    let mut world = idle_world(8, 8);
    let search_npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(3, 3)),
            AiIdleRoam::new(CellCoord::new(3, 3), 0),
            AiSearchForFood,
        ))
        .id();
    let resource = world.spawn_empty().id();
    let gather_npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(4, 4)),
            AiIdleRoam::new(CellCoord::new(4, 4), 0),
            AiGatherResource::new(resource),
        ))
        .id();

    run_idle(&mut world);

    assert!(world.get::<AiIdleRoam>(search_npc).is_none());
    assert!(world.get::<MovementTarget>(search_npc).is_none());
    assert!(world.get::<AiIdleRoam>(gather_npc).is_none());
    assert!(world.get::<MovementTarget>(gather_npc).is_none());
}

#[test]
fn test_idle_roam_yields_to_keep_food_goal_below_target() {
    let origin = CellCoord::new(3, 3);
    let mut world = idle_world(8, 8);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(origin),
            NpcInventory::empty(),
            AiKeepEnoughFoodInInventory::new(DEFAULT_NPC_FOOD_INVENTORY_TARGET),
            AiIdleRoam::new(origin, 0),
            MovementTarget::new(CellCoord::new(4, 3)),
        ))
        .id();

    run_idle(&mut world);

    assert!(world.get::<AiIdleRoam>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_idle_roam_restarts_with_current_position_after_interruption() {
    let mut world = idle_world(8, 8);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            AiIdleRoam::new(CellCoord::new(0, 0), 0),
            AiSearchForFood,
        ))
        .id();

    run_idle(&mut world);
    assert!(world.get::<AiIdleRoam>(npc).is_none());

    world.entity_mut(npc).remove::<AiSearchForFood>();
    world
        .get_mut::<NpcPosition>(npc)
        .expect("NPC should have position")
        .coord = CellCoord::new(4, 4);

    run_idle(&mut world);

    assert_eq!(
        idle_roam(&world, npc)
            .expect("idle roam should restart")
            .origin(),
        CellCoord::new(4, 4)
    );
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
fn test_gather_resource_stops_without_depleting_resource_when_inventory_is_full() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Food, 2);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(2, 1)),
            NpcInventory::new(ResourceAmounts::new(
                DEFAULT_NPC_INVENTORY_MAX_SIZE,
                0,
                0,
                0,
            )),
            AiGatherResource::new(resource),
        ))
        .id();

    for _ in 0..RESOURCE_GATHER_TICKS_PER_UNIT {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_food(&world, npc), 0);
    assert_eq!(resource_quantity(&world, resource), Some(2));
    assert!(world.get::<AiGatherResource>(npc).is_none());
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

fn idle_world(width: usize, height: usize) -> World {
    let mut world = World::new();
    world.insert_resource(Grid::new(width, height));
    world
}

fn spawn_idle_npc(world: &mut World, coord: CellCoord) -> Entity {
    world.spawn((Npc, NpcPosition::new(coord))).id()
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

fn run_idle(world: &mut World) {
    world
        .run_system_once(system_npc_idle)
        .expect("idle system should run");
}

fn idle_roam(world: &World, npc: Entity) -> Option<AiIdleRoam> {
    world.get::<AiIdleRoam>(npc).copied()
}

fn manhattan_distance(a: CellCoord, b: CellCoord) -> u32 {
    a.x().abs_diff(b.x()).saturating_add(a.y().abs_diff(b.y()))
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
