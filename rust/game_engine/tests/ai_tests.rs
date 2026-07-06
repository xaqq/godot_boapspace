use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::ai::{
    system_assign_construction_work, system_deposit_construction_resources, system_gather_resource,
    system_keep_enough_food_in_inventory, system_npc_idle, system_route_construction_work,
    system_search_for_food, AiConstructBuilding, AiGatherResource, AiIdleRoam,
    AiKeepEnoughFoodInInventory, AiSearchForFood, CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE,
    DEFAULT_NPC_FOOD_INVENTORY_TARGET, DEFAULT_NPC_IDLE_DWELL_TICKS, DEFAULT_NPC_IDLE_ROAM_RADIUS,
    RESOURCE_GATHER_TICKS_PER_UNIT,
};
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingFootprint, BuildingKind, ConstructionProgress,
};
use game_engine::components::{
    MaxVelocity, MovementFacing, MovementTarget, NpcInventory, ResourceNode, Tile, TilePosition,
    Velocity, DEFAULT_NPC_INVENTORY_MAX_SIZE,
};
use game_engine::grid::{CellCoord, Grid};
use game_engine::npcs::{Npc, NpcPosition};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::simulation::GameSimulation;
use game_engine::systems::build_surface_schedule;
use game_engine::tasks::maintain_construction_tasks;

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
fn test_idle_roam_is_suppressed_by_construction_work() {
    let mut world = idle_world(8, 8);
    let blueprint = world.spawn_empty().id();
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(3, 3)),
            AiIdleRoam::new(CellCoord::new(3, 3), 0),
            MovementTarget::new(CellCoord::new(4, 3)),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_idle(&mut world);

    assert!(world.get::<AiIdleRoam>(npc).is_none());
    assert_eq!(
        world
            .get::<MovementTarget>(npc)
            .expect("construction movement should remain")
            .coord,
        CellCoord::new(4, 3)
    );
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
fn test_gather_resource_collects_target_node_kind() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Wood, 1);
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(2, 1), resource);

    for _ in 0..RESOURCE_GATHER_TICKS_PER_UNIT {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_resource(&world, npc, ResourceKind::Wood), 1);
    assert_eq!(npc_resource(&world, npc, ResourceKind::Food), 0);
    assert!(world.get::<ResourceNode>(resource).is_none());
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

#[test]
fn test_assign_construction_work_targets_carried_resource_blueprint() {
    let mut world = World::new();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(4, 4));
    world
        .run_system_once(maintain_construction_tasks)
        .expect("task maintenance should run");
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(10, 0, 0, 0)),
        ))
        .id();

    run_assign_construction(&mut world);

    assert_eq!(
        world
            .get::<AiConstructBuilding>(npc)
            .expect("NPC should take construction work")
            .blueprint(),
        blueprint
    );
}

#[test]
fn test_assign_construction_work_yields_to_food_need() {
    let mut world = World::new();
    spawn_construction_blueprint(&mut world, CellCoord::new(4, 4));
    world
        .run_system_once(maintain_construction_tasks)
        .expect("task maintenance should run");
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(10, 0, 0, 0)),
            AiKeepEnoughFoodInInventory::new(DEFAULT_NPC_FOOD_INVENTORY_TARGET),
        ))
        .id();

    run_assign_construction(&mut world);

    assert!(world.get::<AiConstructBuilding>(npc).is_none());
}

#[test]
fn test_route_construction_moves_to_blueprint_before_gathering_when_carrying_needed_resource() {
    let mut world = construction_world();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(4, 4));
    spawn_resource_node(&mut world, CellCoord::new(1, 2), ResourceKind::Wood, 10);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(10, 0, 0, 0)),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_route_construction(&mut world);

    assert_eq!(
        world
            .get::<MovementTarget>(npc)
            .expect("NPC should move to blueprint")
            .coord,
        CellCoord::new(4, 4)
    );
    assert!(world.get::<AiGatherResource>(npc).is_none());
}

#[test]
fn test_deposit_construction_resources_clamps_to_batch_size() {
    let mut world = World::new();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(0, 0));
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(0, 0)),
            NpcInventory::new(ResourceAmounts::new(20, 0, 0, 0)),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_deposit_construction(&mut world);

    assert_eq!(
        construction_progress(&world, blueprint).get(ResourceKind::Wood),
        CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE
    );
    assert_eq!(npc_resource(&world, npc, ResourceKind::Wood), 10);
}

#[test]
fn test_deposit_construction_resources_clamps_to_remaining_cost() {
    let mut world = World::new();
    let blueprint = spawn_construction_blueprint_with_progress(
        &mut world,
        CellCoord::new(0, 0),
        ResourceAmounts::new(35, 0, 0, 0),
    );
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(0, 0)),
            NpcInventory::new(ResourceAmounts::new(20, 0, 0, 0)),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_deposit_construction(&mut world);

    assert_eq!(
        construction_progress(&world, blueprint).get(ResourceKind::Wood),
        40
    );
    assert_eq!(npc_resource(&world, npc, ResourceKind::Wood), 15);
}

#[test]
fn test_deposit_construction_resources_deposits_multiple_needed_kinds() {
    let mut world = World::new();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(0, 0));
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(0, 0)),
            NpcInventory::new(ResourceAmounts::new(10, 10, 0, 0)),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_deposit_construction(&mut world);

    let progress = construction_progress(&world, blueprint);
    assert_eq!(progress.get(ResourceKind::Wood), 10);
    assert_eq!(progress.get(ResourceKind::Stone), 10);
    assert_eq!(npc_resource(&world, npc, ResourceKind::Wood), 0);
    assert_eq!(npc_resource(&world, npc, ResourceKind::Stone), 0);
}

#[test]
fn test_npc_gathers_and_completes_warehouse_construction() {
    let mut world = construction_world();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(0, 0));
    spawn_resource_node(&mut world, CellCoord::new(2, 0), ResourceKind::Wood, 40);
    spawn_resource_node(&mut world, CellCoord::new(0, 2), ResourceKind::Stone, 20);
    world.spawn((
        Npc,
        NpcPosition::new(CellCoord::new(0, 0)),
        Velocity::ZERO,
        MaxVelocity::default(),
        MovementFacing::default(),
        NpcInventory::empty(),
    ));
    let mut schedule = build_surface_schedule();

    for _ in 0..12_000 {
        schedule.run(&mut world);
        if world.get::<Building>(blueprint).is_some() {
            break;
        }
    }

    let building = world
        .get::<Building>(blueprint)
        .expect("warehouse should be completed");
    assert_eq!(building.kind, BuildingKind::Warehouse);
    assert!(world.get::<BuildingBlueprint>(blueprint).is_none());
    assert!(world.get::<ConstructionProgress>(blueprint).is_none());
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

fn spawn_construction_blueprint(world: &mut World, origin: CellCoord) -> Entity {
    spawn_construction_blueprint_with_progress(world, origin, ResourceAmounts::zero())
}

fn spawn_construction_blueprint_with_progress(
    world: &mut World,
    origin: CellCoord,
    deposited: ResourceAmounts,
) -> Entity {
    world
        .spawn((
            BuildingBlueprint {
                kind: BuildingKind::Warehouse,
                footprint: BuildingFootprint::new(origin, 2, 2),
            },
            ConstructionProgress::new(deposited),
        ))
        .id()
}

fn construction_world() -> World {
    let mut world = World::new();
    world.insert_resource(Grid::new(8, 8));
    world
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

fn run_assign_construction(world: &mut World) {
    world
        .run_system_once(system_assign_construction_work)
        .expect("assign-construction system should run");
}

fn run_route_construction(world: &mut World) {
    world
        .run_system_once(system_route_construction_work)
        .expect("route-construction system should run");
}

fn run_deposit_construction(world: &mut World) {
    world
        .run_system_once(system_deposit_construction_resources)
        .expect("deposit-construction system should run");
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
    npc_resource(world, npc, ResourceKind::Food)
}

fn npc_resource(world: &World, npc: Entity, kind: ResourceKind) -> u32 {
    world
        .get::<NpcInventory>(npc)
        .expect("NPC should have inventory")
        .contents()
        .get(kind)
}

fn resource_quantity(world: &World, entity: Entity) -> Option<u32> {
    world
        .get::<ResourceNode>(entity)
        .map(|resource| resource.quantity)
}

fn construction_progress(world: &World, entity: Entity) -> ResourceAmounts {
    world
        .get::<ConstructionProgress>(entity)
        .expect("blueprint should have construction progress")
        .deposited()
}
