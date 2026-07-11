use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::ai::{
    system_assign_construction_work, system_deposit_construction_resources, system_gather_resource,
    system_npc_idle, system_route_construction_work, AiConstructBuilding, AiGatherResource,
    AiIdleRoam, AiKeepEnoughFoodInInventory, AiSearchForFood,
    CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE, DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD,
    DEFAULT_NPC_FOOD_INVENTORY_TARGET, DEFAULT_NPC_IDLE_DWELL_TICKS, DEFAULT_NPC_IDLE_ROAM_RADIUS,
    RESOURCE_GATHER_TICKS_PER_UNIT,
};
use game_engine::buildings::{
    system_complete_building_construction, Building, BuildingBlueprint, BuildingFootprint,
    BuildingKind, ConstructionProgress, WarehouseInventory,
};
use game_engine::components::{
    CarriedResource, FoodPouch, MaxVelocity, MovementFacing, MovementTarget, NpcInventory,
    ResourceNode, TerrainKind, Tile, TilePosition, Velocity, DEFAULT_NPC_INVENTORY_MAX_SIZE,
};
use game_engine::grid::{CellCoord, Grid};
use game_engine::housing::{House, HousingAssignment};
use game_engine::logistics::manage_food_logistics;
use game_engine::navigation::{refresh_navigation_snapshot, NpcRoute};
use game_engine::npcs::{Npc, NpcPosition, NpcSkills, SkillKind};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::simulation::GameSimulation;
use game_engine::systems::build_surface_schedule;
use game_engine::tasks::maintain_construction_tasks;
use game_engine::tile::{TileBundle, TileIndex};

#[test]
fn test_initial_npc_has_keep_food_goal() {
    let simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();

    let thresholds = simulation
        .with_surface_world(surface, |world| {
            let mut query = world.try_query::<(&AiKeepEnoughFoodInInventory, &Npc)>()?;
            query
                .iter(world)
                .next()
                .map(|(goal, _)| (goal.start_threshold(), goal.target()))
        })
        .expect("default NPC should have keep-food goal");

    assert_eq!(
        thresholds,
        (
            DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD,
            DEFAULT_NPC_FOOD_INVENTORY_TARGET
        )
    );
}

#[test]
fn test_keep_enough_food_adds_search_when_at_start_threshold() {
    let mut world = construction_world();
    spawn_food_warehouse(&mut world, CellCoord::new(3, 1), 10);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(
                0,
                0,
                DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD,
                0,
            )),
            default_keep_food_goal(),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_some());
}

#[test]
fn test_keep_enough_food_does_not_search_inside_food_buffer() {
    let mut world = construction_world();
    spawn_food_warehouse(&mut world, CellCoord::new(3, 1), 10);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(
                0,
                0,
                DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD + 1,
                0,
            )),
            default_keep_food_goal(),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_none());
}

#[test]
fn test_keep_enough_food_ignores_full_legacy_inventory() {
    let mut world = construction_world();
    spawn_food_warehouse(&mut world, CellCoord::new(3, 1), 10);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(
                DEFAULT_NPC_INVENTORY_MAX_SIZE,
                0,
                0,
                0,
            )),
            default_keep_food_goal(),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_some());
}

#[test]
fn test_keep_enough_food_does_not_add_search_when_at_target() {
    let mut world = construction_world();
    spawn_food_warehouse(&mut world, CellCoord::new(3, 1), 10);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(
                0,
                0,
                DEFAULT_NPC_FOOD_INVENTORY_TARGET,
                0,
            )),
            default_keep_food_goal(),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_none());
}

#[test]
fn test_keep_enough_food_removes_active_search_when_target_is_reached() {
    let mut world = construction_world();
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(
                0,
                0,
                DEFAULT_NPC_FOOD_INVENTORY_TARGET,
                0,
            )),
            default_keep_food_goal(),
            AiSearchForFood,
            MovementTarget::new(CellCoord::new(1, 1)),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_keep_enough_food_does_not_search_when_no_food_exists() {
    let mut world = construction_world();
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::empty(),
            default_keep_food_goal(),
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

    let route = world
        .get::<NpcRoute>(npc)
        .expect("idle roam should queue a route");
    let target = route.goals()[0];
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
            .get::<NpcRoute>(npc)
            .expect("idle roam should choose a valid route")
            .goals()[0],
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
    let mut world = construction_world();
    spawn_food_warehouse(&mut world, CellCoord::new(5, 3), 10);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(origin),
            NpcInventory::empty(),
            default_keep_food_goal(),
            AiIdleRoam::new(origin, 0),
            MovementTarget::new(CellCoord::new(4, 3)),
        ))
        .id();

    run_keep_enough_food(&mut world);
    run_idle(&mut world);

    assert!(world.get::<AiIdleRoam>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_idle_roam_continues_when_food_is_low_but_unavailable() {
    let origin = CellCoord::new(3, 3);
    let mut world = idle_world(8, 8);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(origin),
            NpcInventory::empty(),
            default_keep_food_goal(),
            AiIdleRoam::new(origin, 0),
            MovementTarget::new(CellCoord::new(4, 3)),
        ))
        .id();

    run_idle(&mut world);

    assert!(world.get::<AiIdleRoam>(npc).is_some());
    assert_eq!(
        world
            .get::<MovementTarget>(npc)
            .expect("idle movement should continue")
            .coord,
        CellCoord::new(4, 3)
    );
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
fn test_idle_roam_uses_assigned_house_as_its_anchor() {
    let mut world = idle_world(12, 12);
    let footprint = BuildingFootprint::new(CellCoord::new(6, 6), 2, 2);
    let house = spawn_house(&mut world, BuildingKind::MediumHouse, footprint, 0);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            HousingAssignment::new(house, 0),
        ))
        .id();

    run_idle(&mut world);

    let idle = idle_roam(&world, npc).expect("resident should start idle roaming");
    assert_eq!(idle.house(), Some(house));
    assert_eq!(idle.house_slot(), Some(0));
    assert_eq!(idle.origin(), footprint.origin());
    assert_eq!(idle.dwell_ticks_remaining(), DEFAULT_NPC_IDLE_DWELL_TICKS);
}

#[test]
fn test_idle_roam_targets_home_zone_for_every_house_size() {
    for (kind, footprint) in [
        (
            BuildingKind::SmallHouse,
            BuildingFootprint::new(CellCoord::new(5, 5), 1, 1),
        ),
        (
            BuildingKind::MediumHouse,
            BuildingFootprint::new(CellCoord::new(5, 5), 2, 2),
        ),
        (
            BuildingKind::LargeHouse,
            BuildingFootprint::new(CellCoord::new(5, 5), 3, 3),
        ),
    ] {
        let mut world = idle_world(14, 14);
        let house = spawn_house(&mut world, kind, footprint, 0);
        let npc = world
            .spawn((
                Npc,
                NpcPosition::new(CellCoord::new(0, 0)),
                HousingAssignment::new(house, 0),
                AiIdleRoam::around_house(footprint.origin(), house, 0, 1, 0),
            ))
            .id();

        run_idle(&mut world);

        let target = world
            .get::<NpcRoute>(npc)
            .expect("resident should route toward its home zone")
            .goals()[0];
        let distance = footprint_distance(footprint, target);
        assert!(!footprint.contains(target));
        assert!((1..=DEFAULT_NPC_IDLE_ROAM_RADIUS).contains(&distance));
    }
}

#[test]
fn test_idle_roam_staggers_housemates_by_housing_slot() {
    let mut world = idle_world(12, 12);
    let footprint = BuildingFootprint::new(CellCoord::new(5, 5), 2, 2);
    let house = spawn_house(&mut world, BuildingKind::MediumHouse, footprint, 0);
    let residents = (0..4)
        .map(|slot| {
            world
                .spawn((
                    Npc,
                    NpcPosition::new(CellCoord::new(1, 1)),
                    HousingAssignment::new(house, slot),
                ))
                .id()
        })
        .collect::<Vec<_>>();

    run_idle(&mut world);

    let offsets = residents
        .iter()
        .map(|resident| idle_roam(&world, *resident).unwrap().next_offset_index())
        .collect::<Vec<_>>();
    assert_eq!(offsets, vec![0, 9, 18, 27]);
}

#[test]
fn test_idle_roam_skips_blocked_home_zone_candidate() {
    let mut world = idle_world(10, 10);
    let footprint = BuildingFootprint::new(CellCoord::new(4, 4), 1, 1);
    let house = spawn_house(&mut world, BuildingKind::SmallHouse, footprint, 0);
    let blocked = CellCoord::new(4, 3);
    spawn_resource_node(&mut world, blocked, ResourceKind::Wood, 1);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            HousingAssignment::new(house, 0),
            AiIdleRoam::around_house(footprint.origin(), house, 0, 1, 0),
        ))
        .id();

    run_idle(&mut world);

    let target = world.get::<NpcRoute>(npc).unwrap().goals()[0];
    assert_ne!(target, blocked);
    assert!((1..=DEFAULT_NPC_IDLE_ROAM_RADIUS).contains(&footprint_distance(footprint, target)));
}

#[test]
fn test_idle_roam_waits_when_home_zone_is_unreachable() {
    let mut world = idle_world(10, 9);
    let footprint = BuildingFootprint::new(CellCoord::new(7, 4), 1, 1);
    let house = spawn_house(&mut world, BuildingKind::SmallHouse, footprint, 0);
    for y in 0..9 {
        spawn_resource_node(&mut world, CellCoord::new(4, y), ResourceKind::Wood, 1);
    }
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 4)),
            HousingAssignment::new(house, 0),
            AiIdleRoam::around_house(footprint.origin(), house, 0, 1, 0),
        ))
        .id();

    run_idle(&mut world);

    assert!(world.get::<NpcRoute>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
    assert_eq!(
        idle_roam(&world, npc).unwrap().dwell_ticks_remaining(),
        DEFAULT_NPC_IDLE_DWELL_TICKS
    );
}

#[test]
fn test_idle_roam_waits_when_house_has_no_home_zone_cells() {
    let mut world = idle_world(1, 1);
    let footprint = BuildingFootprint::new(CellCoord::new(0, 0), 1, 1);
    let house = spawn_house(&mut world, BuildingKind::SmallHouse, footprint, 0);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(0, 0)),
            HousingAssignment::new(house, 0),
            AiIdleRoam::around_house(footprint.origin(), house, 0, 1, 0),
        ))
        .id();

    run_idle(&mut world);

    assert!(world.get::<NpcRoute>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
    assert_eq!(
        idle_roam(&world, npc).unwrap().dwell_ticks_remaining(),
        DEFAULT_NPC_IDLE_DWELL_TICKS
    );
}

#[test]
fn test_idle_roam_retargets_when_house_assignment_changes() {
    let mut world = idle_world(12, 12);
    let old_footprint = BuildingFootprint::new(CellCoord::new(2, 2), 1, 1);
    let new_footprint = BuildingFootprint::new(CellCoord::new(8, 8), 1, 1);
    let old_house = spawn_house(&mut world, BuildingKind::SmallHouse, old_footprint, 0);
    let new_house = spawn_house(&mut world, BuildingKind::SmallHouse, new_footprint, 1);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(4, 4)),
            HousingAssignment::new(new_house, 0),
            AiIdleRoam::around_house(old_footprint.origin(), old_house, 0, 0, 0),
            NpcRoute::to_cell(CellCoord::new(2, 1)),
            MovementTarget::new(CellCoord::new(2, 1)),
        ))
        .id();

    run_idle(&mut world);

    let idle = idle_roam(&world, npc).unwrap();
    assert_eq!(idle.house(), Some(new_house));
    assert_eq!(idle.origin(), new_footprint.origin());
    assert_eq!(idle.dwell_ticks_remaining(), DEFAULT_NPC_IDLE_DWELL_TICKS);
    assert!(world.get::<NpcRoute>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_idle_roam_rephases_when_housing_slot_changes() {
    let mut world = idle_world(12, 12);
    let footprint = BuildingFootprint::new(CellCoord::new(5, 5), 2, 2);
    let house = spawn_house(&mut world, BuildingKind::MediumHouse, footprint, 0);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(3, 3)),
            HousingAssignment::new(house, 3),
            AiIdleRoam::around_house(footprint.origin(), house, 0, 0, 0),
            NpcRoute::to_cell(CellCoord::new(5, 4)),
            MovementTarget::new(CellCoord::new(5, 4)),
        ))
        .id();

    run_idle(&mut world);

    let idle = idle_roam(&world, npc).unwrap();
    assert_eq!(idle.house(), Some(house));
    assert_eq!(idle.house_slot(), Some(3));
    assert_eq!(idle.next_offset_index(), 27);
    assert!(world.get::<NpcRoute>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_idle_roam_falls_back_locally_when_assigned_house_is_removed() {
    let mut world = idle_world(10, 10);
    let footprint = BuildingFootprint::new(CellCoord::new(6, 6), 1, 1);
    let house = spawn_house(&mut world, BuildingKind::SmallHouse, footprint, 0);
    let position = CellCoord::new(3, 3);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(position),
            HousingAssignment::new(house, 0),
            AiIdleRoam::around_house(footprint.origin(), house, 0, 0, 0),
            NpcRoute::to_cell(CellCoord::new(6, 5)),
            MovementTarget::new(CellCoord::new(6, 5)),
        ))
        .id();
    world.despawn(house);

    run_idle(&mut world);

    let idle = idle_roam(&world, npc).unwrap();
    assert_eq!(idle.house(), None);
    assert_eq!(idle.origin(), position);
    assert!(world.get::<NpcRoute>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_food_logistics_routes_to_nearest_cooked_food_inventory() {
    let mut world = construction_world();
    let npc = spawn_searching_npc(&mut world, CellCoord::new(1, 1));
    spawn_resource_node(
        &mut world,
        CellCoord::new(1, 2),
        ResourceKind::WildBerries,
        10,
    );
    spawn_food_warehouse(&mut world, CellCoord::new(6, 5), 10);
    let nearest = spawn_food_warehouse(&mut world, CellCoord::new(4, 1), 10);

    run_search_for_food(&mut world);

    let route = world
        .get::<NpcRoute>(npc)
        .expect("food logistics should queue a collision-aware route");
    assert!(route.goals().contains(&CellCoord::new(3, 1)));
    assert!(!route.goals().contains(&CellCoord::new(4, 1)));
    assert_eq!(
        world.get::<Building>(nearest).unwrap().kind,
        BuildingKind::Warehouse
    );
    assert!(world.get::<AiSearchForFood>(npc).is_some());
}

#[test]
fn test_search_for_food_stops_when_no_food_exists() {
    let mut world = construction_world();
    let npc = spawn_searching_npc(&mut world, CellCoord::new(1, 1));
    spawn_resource_node(
        &mut world,
        CellCoord::new(1, 2),
        ResourceKind::WildBerries,
        10,
    );

    run_search_for_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_none());
    assert!(world.get::<MovementTarget>(npc).is_none());
}

#[test]
fn test_full_legacy_inventory_does_not_block_food_pouch_refill() {
    let mut world = construction_world();
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(
                DEFAULT_NPC_INVENTORY_MAX_SIZE,
                0,
                0,
                0,
            )),
            default_keep_food_goal(),
            AiSearchForFood,
            MovementTarget::new(CellCoord::new(2, 1)),
        ))
        .id();
    spawn_food_warehouse(&mut world, CellCoord::new(3, 1), 10);

    run_search_for_food(&mut world);

    assert!(world.get::<AiSearchForFood>(npc).is_some());
    assert_eq!(npc_food(&world, npc), 0);
}

#[test]
fn test_food_logistics_withdraws_immediately_from_adjacent_warehouse() {
    let mut world = construction_world();
    let npc = spawn_searching_npc(&mut world, CellCoord::new(2, 1));
    let warehouse = spawn_food_warehouse(
        &mut world,
        CellCoord::new(3, 1),
        DEFAULT_NPC_FOOD_INVENTORY_TARGET,
    );

    run_search_for_food(&mut world);

    assert_eq!(npc_food(&world, npc), DEFAULT_NPC_FOOD_INVENTORY_TARGET);
    assert!(world.get::<AiSearchForFood>(npc).is_none());
    assert_eq!(
        world
            .get::<WarehouseInventory>(warehouse)
            .unwrap()
            .contents()
            .get(ResourceKind::Food),
        0
    );
}

#[test]
fn test_raw_wild_berries_do_not_interrupt_existing_gather_work_for_food() {
    let mut world = construction_world();
    let resource = spawn_resource_node(
        &mut world,
        CellCoord::new(2, 1),
        ResourceKind::WildBerries,
        10,
    );
    let mut gather = AiGatherResource::new(resource);
    gather.advance_tick();
    gather.advance_tick();
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            FoodPouch::new(DEFAULT_NPC_FOOD_INVENTORY_TARGET),
            CarriedResource::empty(),
            default_keep_food_goal(),
            gather,
        ))
        .id();

    run_keep_enough_food(&mut world);

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
    let resource = spawn_resource_node(
        &mut world,
        CellCoord::new(2, 1),
        ResourceKind::WildBerries,
        2,
    );
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(1, 1), resource);

    for _ in 0..(RESOURCE_GATHER_TICKS_PER_UNIT - 1) {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_resource(&world, npc, ResourceKind::WildBerries), 0);
    assert_eq!(
        world
            .get::<AiGatherResource>(npc)
            .expect("gather should still be active")
            .progress_ticks(),
        RESOURCE_GATHER_TICKS_PER_UNIT - 1
    );
    assert_eq!(resource_quantity(&world, resource), Some(2));

    run_gather_resource(&mut world);

    assert_eq!(npc_resource(&world, npc, ResourceKind::WildBerries), 1);
    assert_eq!(npc_skill(&world, npc, SkillKind::Forager), 1);
    assert!(world.get::<AiGatherResource>(npc).is_none());
    assert_eq!(resource_quantity(&world, resource), Some(1));
}

#[test]
fn test_successful_gathers_increment_only_matching_skill() {
    let cases = [
        (ResourceKind::Wood, SkillKind::Lumberjack),
        (ResourceKind::Stone, SkillKind::Quarryman),
        (ResourceKind::WildBerries, SkillKind::Forager),
        (ResourceKind::Gold, SkillKind::Prospector),
    ];

    for (resource_kind, expected_skill) in cases {
        let mut world = World::new();
        let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), resource_kind, 1);
        let npc = spawn_gathering_npc(&mut world, CellCoord::new(1, 1), resource);

        for _ in 0..RESOURCE_GATHER_TICKS_PER_UNIT {
            run_gather_resource(&mut world);
        }

        for skill in SkillKind::ALL {
            let expected_value = u32::from(skill == expected_skill);
            assert_eq!(
                npc_skill(&world, npc, skill),
                expected_value,
                "{resource_kind:?} should only increment {expected_skill:?}"
            );
        }
    }
}

#[test]
fn test_partial_gather_progress_awards_no_skill_xp() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Wood, 1);
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(1, 1), resource);

    for _ in 0..(RESOURCE_GATHER_TICKS_PER_UNIT - 1) {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_skill(&world, npc, SkillKind::Lumberjack), 0);
    assert_eq!(resource_quantity(&world, resource), Some(1));
}

#[test]
fn test_food_refill_withdraws_cooked_food_to_target_in_one_atomic_transfer() {
    let mut world = construction_world();
    spawn_food_warehouse(
        &mut world,
        CellCoord::new(3, 1),
        DEFAULT_NPC_FOOD_INVENTORY_TARGET,
    );
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(2, 1)),
            NpcInventory::new(ResourceAmounts::new(
                0,
                0,
                DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD,
                0,
            )),
            default_keep_food_goal(),
        ))
        .id();

    run_keep_enough_food(&mut world);

    assert_eq!(npc_food(&world, npc), DEFAULT_NPC_FOOD_INVENTORY_TARGET);
    assert!(world.get::<AiSearchForFood>(npc).is_none());
    assert!(world.get::<AiGatherResource>(npc).is_none());
}

#[test]
fn test_gather_resource_removes_depleted_resource_node_not_tile() {
    let mut world = World::new();
    let resource = spawn_resource_node(
        &mut world,
        CellCoord::new(2, 1),
        ResourceKind::WildBerries,
        1,
    );
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(1, 1), resource);

    for _ in 0..RESOURCE_GATHER_TICKS_PER_UNIT {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_resource(&world, npc, ResourceKind::WildBerries), 1);
    assert!(world.get::<ResourceNode>(resource).is_none());
    assert!(world.get::<TilePosition>(resource).is_some());
    assert!(world.get::<Tile>(resource).is_some());
}

#[test]
fn test_gather_resource_stops_without_depleting_resource_when_inventory_is_full() {
    let mut world = World::new();
    let resource = spawn_resource_node(
        &mut world,
        CellCoord::new(2, 1),
        ResourceKind::WildBerries,
        2,
    );
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::new(
                DEFAULT_NPC_INVENTORY_MAX_SIZE,
                0,
                0,
                0,
            )),
            NpcSkills::default(),
            AiGatherResource::new(resource),
        ))
        .id();

    for _ in 0..RESOURCE_GATHER_TICKS_PER_UNIT {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_food(&world, npc), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Forager), 0);
    assert_eq!(resource_quantity(&world, resource), Some(2));
    assert!(world.get::<AiGatherResource>(npc).is_none());
}

#[test]
fn test_gather_resource_collects_target_node_kind() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Wood, 1);
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(1, 1), resource);

    for _ in 0..RESOURCE_GATHER_TICKS_PER_UNIT {
        run_gather_resource(&mut world);
    }

    assert_eq!(npc_resource(&world, npc, ResourceKind::Wood), 1);
    assert_eq!(npc_resource(&world, npc, ResourceKind::Food), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Lumberjack), 1);
    assert_eq!(npc_skill(&world, npc, SkillKind::Forager), 0);
    assert!(world.get::<ResourceNode>(resource).is_none());
}

#[test]
fn test_gather_resource_removes_invalid_target_without_awarding_food() {
    let mut world = World::new();
    let stale_target = world.spawn_empty().id();
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(2, 1), stale_target);

    run_gather_resource(&mut world);

    assert_eq!(npc_food(&world, npc), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Forager), 0);
    assert!(world.get::<AiGatherResource>(npc).is_none());
}

#[test]
fn test_gather_resource_removes_moved_away_gather_without_awarding_food() {
    let mut world = World::new();
    let resource = spawn_resource_node(
        &mut world,
        CellCoord::new(2, 1),
        ResourceKind::WildBerries,
        1,
    );
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(4, 1), resource);

    run_gather_resource(&mut world);

    assert_eq!(npc_food(&world, npc), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Forager), 0);
    assert!(world.get::<AiGatherResource>(npc).is_none());
    assert_eq!(resource_quantity(&world, resource), Some(1));
}

#[test]
fn test_gather_resource_removes_depleted_target_without_skill_xp() {
    let mut world = World::new();
    let resource = spawn_resource_node(&mut world, CellCoord::new(2, 1), ResourceKind::Stone, 0);
    let npc = spawn_gathering_npc(&mut world, CellCoord::new(1, 1), resource);

    run_gather_resource(&mut world);

    assert_eq!(npc_skill(&world, npc, SkillKind::Quarryman), 0);
    assert!(world.get::<AiGatherResource>(npc).is_none());
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
            NpcInventory::new(ResourceAmounts::of(ResourceKind::Planks, 10)),
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
    let mut world = construction_world();
    spawn_construction_blueprint(&mut world, CellCoord::new(4, 4));
    spawn_food_warehouse(&mut world, CellCoord::new(3, 1), 10);
    world
        .run_system_once(maintain_construction_tasks)
        .expect("task maintenance should run");
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::of(ResourceKind::Planks, 10)),
            default_keep_food_goal(),
        ))
        .id();

    run_keep_enough_food(&mut world);
    run_assign_construction(&mut world);

    assert!(world.get::<AiConstructBuilding>(npc).is_none());
}

#[test]
fn test_assign_construction_work_ignores_food_buffer_above_start_threshold() {
    let mut world = construction_world();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(4, 4));
    spawn_food_warehouse(&mut world, CellCoord::new(3, 1), 10);
    world
        .run_system_once(maintain_construction_tasks)
        .expect("task maintenance should run");
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::of(ResourceKind::Planks, 10).with(
                ResourceKind::Food,
                DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD + 1,
            )),
            default_keep_food_goal(),
        ))
        .id();

    run_assign_construction(&mut world);

    assert_eq!(
        world
            .get::<AiConstructBuilding>(npc)
            .expect("NPC should take construction work while in the food buffer")
            .blueprint(),
        blueprint
    );
}

#[test]
fn test_assign_construction_work_continues_when_food_is_low_but_unavailable() {
    let mut world = World::new();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(4, 4));
    world
        .run_system_once(maintain_construction_tasks)
        .expect("task maintenance should run");
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::of(ResourceKind::Planks, 10)),
            default_keep_food_goal(),
        ))
        .id();

    run_assign_construction(&mut world);

    assert_eq!(
        world
            .get::<AiConstructBuilding>(npc)
            .expect("NPC should keep working when food cannot be collected")
            .blueprint(),
        blueprint
    );
}

#[test]
fn test_route_construction_moves_to_blueprint_before_gathering_when_carrying_needed_resource() {
    let mut world = construction_world();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(4, 4));
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::new(ResourceAmounts::of(ResourceKind::Planks, 10)),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_route_construction(&mut world);

    let route = world
        .get::<NpcRoute>(npc)
        .expect("NPC should route to an exterior blueprint interaction cell");
    assert!(route
        .goals()
        .iter()
        .all(|goal| !BuildingFootprint::new(CellCoord::new(4, 4), 2, 2).contains(*goal)));
    assert!(route.goals().contains(&CellCoord::new(4, 3)));
    assert!(world.get::<AiGatherResource>(npc).is_none());
}

#[test]
fn test_deposit_construction_resources_clamps_to_batch_size() {
    let mut world = World::new();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(0, 0));
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(2, 0)),
            NpcInventory::new(ResourceAmounts::of(ResourceKind::Planks, 20)),
            NpcSkills::default(),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_deposit_construction(&mut world);

    assert_eq!(
        construction_progress(&world, blueprint).get(ResourceKind::Planks),
        CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE
    );
    assert_eq!(npc_resource(&world, npc, ResourceKind::Planks), 10);
    assert_eq!(npc_skill(&world, npc, SkillKind::Builder), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);
}

#[test]
fn test_deposit_construction_resources_clamps_to_remaining_cost() {
    let mut world = World::new();
    let blueprint = spawn_construction_blueprint_with_progress(
        &mut world,
        CellCoord::new(0, 0),
        ResourceAmounts::of(ResourceKind::Planks, 35),
    );
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(2, 0)),
            NpcInventory::new(ResourceAmounts::of(ResourceKind::Planks, 20)),
            NpcSkills::default(),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_deposit_construction(&mut world);

    assert_eq!(
        construction_progress(&world, blueprint).get(ResourceKind::Planks),
        40
    );
    assert_eq!(npc_resource(&world, npc, ResourceKind::Planks), 15);
    assert_eq!(npc_skill(&world, npc, SkillKind::Builder), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);
}

#[test]
fn test_deposit_construction_resources_deposits_multiple_needed_kinds() {
    let mut world = World::new();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(0, 0));
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(2, 0)),
            NpcInventory::new(
                ResourceAmounts::of(ResourceKind::Planks, 10).with(ResourceKind::StoneBlocks, 10),
            ),
            NpcSkills::default(),
            AiConstructBuilding::new(blueprint),
        ))
        .id();

    run_deposit_construction(&mut world);

    let progress = construction_progress(&world, blueprint);
    assert_eq!(progress.get(ResourceKind::Planks), 10);
    assert_eq!(progress.get(ResourceKind::StoneBlocks), 10);
    assert_eq!(npc_resource(&world, npc, ResourceKind::Planks), 0);
    assert_eq!(npc_resource(&world, npc, ResourceKind::StoneBlocks), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Builder), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);
}

#[test]
fn test_building_completion_does_not_award_builder_or_farmer_xp() {
    let mut world = World::new();
    let npc = world.spawn((Npc, NpcSkills::default())).id();
    let blueprint = spawn_construction_blueprint_with_progress(
        &mut world,
        CellCoord::new(0, 0),
        BuildingKind::Warehouse.definition().construction_cost(),
    );

    world
        .run_system_once(system_complete_building_construction)
        .expect("building completion system should run");

    assert!(world.get::<Building>(blueprint).is_some());
    assert_eq!(npc_skill(&world, npc, SkillKind::Builder), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);
}

#[test]
fn test_npc_hauls_refined_materials_and_completes_warehouse_construction() {
    let mut world = construction_world();
    let blueprint = spawn_construction_blueprint(&mut world, CellCoord::new(0, 0));
    let source = world
        .spawn((
            Building::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(5, 5), 2, 2),
            ),
            WarehouseInventory::empty(),
        ))
        .id();
    {
        let mut inventory = world.get_mut::<WarehouseInventory>(source).unwrap();
        assert!(inventory.add(ResourceKind::Planks, 40));
        assert!(inventory.add(ResourceKind::StoneBlocks, 20));
    }
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(4, 5)),
            Velocity::ZERO,
            MaxVelocity::default(),
            MovementFacing::default(),
            FoodPouch::new(DEFAULT_NPC_FOOD_INVENTORY_TARGET),
            CarriedResource::empty(),
            NpcSkills::default(),
        ))
        .id();
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
    assert_eq!(npc_skill(&world, npc, SkillKind::Builder), 0);
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);
}

fn spawn_searching_npc(world: &mut World, coord: CellCoord) -> Entity {
    world
        .spawn((
            Npc,
            NpcPosition::new(coord),
            NpcInventory::empty(),
            default_keep_food_goal(),
            AiSearchForFood,
        ))
        .id()
}

fn spawn_gathering_npc(world: &mut World, coord: CellCoord, target: Entity) -> Entity {
    world
        .spawn((
            Npc,
            NpcPosition::new(coord),
            NpcInventory::empty(),
            NpcSkills::default(),
            AiGatherResource::new(target),
        ))
        .id()
}

fn idle_world(width: usize, height: usize) -> World {
    world_with_tiles(width, height)
}

fn world_with_tiles(width: usize, height: usize) -> World {
    let mut world = World::new();
    let grid = Grid::new(width, height);
    let mut tile_index = TileIndex::new(grid.size());
    for coord in grid.size().iter_coords() {
        let tile = world
            .spawn(TileBundle::new_with_terrain(coord, TerrainKind::Grass))
            .id();
        assert!(tile_index.set(coord, tile));
    }
    world.insert_resource(grid);
    world.insert_resource(tile_index);
    world
}

fn spawn_idle_npc(world: &mut World, coord: CellCoord) -> Entity {
    world.spawn((Npc, NpcPosition::new(coord))).id()
}

fn spawn_house(
    world: &mut World,
    kind: BuildingKind,
    footprint: BuildingFootprint,
    completion_order: u64,
) -> Entity {
    let capacity = kind
        .definition()
        .housing_capacity()
        .expect("test building kind should provide housing");
    world
        .spawn((
            Building::new(kind, footprint),
            House::new(capacity, completion_order),
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
    world_with_tiles(8, 8)
}

fn spawn_food_warehouse(world: &mut World, origin: CellCoord, amount: u32) -> Entity {
    let mut inventory = WarehouseInventory::empty();
    assert!(inventory.add(ResourceKind::Food, amount));
    world
        .spawn((
            Building::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(origin, 2, 2),
            ),
            inventory,
        ))
        .id()
}

fn run_keep_enough_food(world: &mut World) {
    ensure_live_npc_containers(world);
    manage_food_logistics(world);
}

fn run_search_for_food(world: &mut World) {
    ensure_live_npc_containers(world);
    manage_food_logistics(world);
}

fn ensure_live_npc_containers(world: &mut World) {
    let mut query = world.query::<(Entity, &NpcInventory)>();
    let missing = query
        .iter(world)
        .filter(|(entity, _)| world.get::<FoodPouch>(*entity).is_none())
        .map(|(entity, inventory)| {
            (
                entity,
                FoodPouch::new(inventory.contents().get(ResourceKind::Food)),
            )
        })
        .collect::<Vec<_>>();
    for (entity, pouch) in missing {
        world
            .entity_mut(entity)
            .insert((pouch, CarriedResource::empty()));
    }
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
    refresh_navigation_snapshot(world);
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

fn footprint_distance(footprint: BuildingFootprint, coord: CellCoord) -> u32 {
    footprint
        .iter_coords()
        .map(|footprint_coord| manhattan_distance(footprint_coord, coord))
        .min()
        .expect("test footprints are non-empty")
}

fn npc_food(world: &World, npc: Entity) -> u32 {
    world.get::<FoodPouch>(npc).map_or_else(
        || npc_resource(world, npc, ResourceKind::Food),
        |pouch| pouch.amount(),
    )
}

fn npc_resource(world: &World, npc: Entity, kind: ResourceKind) -> u32 {
    world
        .get::<NpcInventory>(npc)
        .expect("NPC should have inventory")
        .contents()
        .get(kind)
}

fn npc_skill(world: &World, npc: Entity, kind: SkillKind) -> u32 {
    world
        .get::<NpcSkills>(npc)
        .expect("NPC should have skills")
        .value(kind)
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

fn default_keep_food_goal() -> AiKeepEnoughFoodInInventory {
    AiKeepEnoughFoodInInventory::new(
        DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD,
        DEFAULT_NPC_FOOD_INVENTORY_TARGET,
    )
}
