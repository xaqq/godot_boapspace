use bevy_ecs::prelude::*;
use game_engine::buildings::{
    Building, BuildingActivity, BuildingBlueprint, BuildingFootprint, BuildingKind,
    ConstructionProgress, RefineryPullConfig, StorageInventory, StoragePullConfig,
};
use game_engine::components::{
    AiConstructBuilding, CarriedResource, Npc, NpcPosition, Wheelbarrow,
};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::logistics::{
    cancel_work_involving_building, manage_building_logistics, manage_wheelbarrow_recovery,
    AiBuildingHaul, AiWheelbarrowRecovery, BuildingHaulPhase,
};
use game_engine::navigation::{refresh_navigation_snapshot, NpcRoute};
use game_engine::refining::{RefineryInventory, ReservationLedger, StockEndpoint};
use game_engine::resources::{resource_overview, ResourceKind};
use game_engine::tasks::{manage_construction_labor, AiConstructionLabor};
use game_engine::tile::{TileBundle, TileIndex};

#[test]
fn storage_pull_uses_a_twenty_five_unit_wheelbarrow_and_delivers() {
    let mut world = navigation_world();
    let source = spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(2, 2));
    assert!(world
        .get_mut::<RefineryInventory>(source)
        .unwrap()
        .add_output(BuildingKind::Sawmill, ResourceKind::Planks, 40));
    let storage = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(5, 2));
    world
        .get_mut::<StoragePullConfig>(storage)
        .unwrap()
        .set_pulls_from_refineries(ResourceKind::Planks, true);
    let worker = spawn_worker(&mut world, CellCoord::new(1, 2));

    manage_building_logistics(&mut world);
    let haul = *world.get::<AiBuildingHaul>(worker).unwrap();
    assert_eq!(haul.source(), StockEndpoint::RefineryOutput(source));
    assert_eq!(haul.amount(), 25);
    assert!(haul.uses_wheelbarrow());
    assert_eq!(world.get::<Wheelbarrow>(worker).unwrap().stack(), None);

    manage_building_logistics(&mut world);
    assert_eq!(
        world.get::<AiBuildingHaul>(worker).unwrap().phase(),
        BuildingHaulPhase::ToSink
    );
    let load = world.get::<Wheelbarrow>(worker).unwrap().stack().unwrap();
    assert_eq!((load.kind(), load.amount()), (ResourceKind::Planks, 25));

    world
        .get_mut::<StoragePullConfig>(storage)
        .unwrap()
        .set_pulls_from_refineries(ResourceKind::Planks, false);
    world.get_mut::<NpcPosition>(worker).unwrap().coord = CellCoord::new(4, 2);
    manage_building_logistics(&mut world);

    assert!(world.get::<AiBuildingHaul>(worker).is_none());
    assert!(world.get::<Wheelbarrow>(worker).is_none());
    assert_eq!(
        world
            .get::<StorageInventory>(storage)
            .unwrap()
            .contents()
            .get(ResourceKind::Planks),
        25
    );
    assert_eq!(
        world
            .get::<RefineryInventory>(source)
            .unwrap()
            .output_contents()
            .get(ResourceKind::Planks),
        15
    );
}

#[test]
fn refinery_storage_pull_mode_excludes_natural_sources() {
    let mut world = navigation_world();
    let storage = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(2, 2));
    assert!(world
        .get_mut::<StorageInventory>(storage)
        .unwrap()
        .add(ResourceKind::Wood, 30));
    let refinery = spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(6, 2));
    world
        .get_mut::<RefineryPullConfig>(refinery)
        .unwrap()
        .set_pulls_from_storage(ResourceKind::Wood, true);
    let worker = spawn_worker(&mut world, CellCoord::new(4, 2));

    manage_building_logistics(&mut world);

    let haul = *world.get::<AiBuildingHaul>(worker).unwrap();
    assert_eq!(haul.source(), StockEndpoint::Warehouse(storage));
    assert!(haul.uses_wheelbarrow());
}

#[test]
fn inactive_endpoints_do_not_create_logistics_work() {
    let mut world = navigation_world();
    let source = spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(2, 2));
    assert!(world
        .get_mut::<RefineryInventory>(source)
        .unwrap()
        .add_output(BuildingKind::Sawmill, ResourceKind::Planks, 20));
    world
        .get_mut::<BuildingActivity>(source)
        .unwrap()
        .set_active(false);
    let storage = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(5, 2));
    world
        .get_mut::<StoragePullConfig>(storage)
        .unwrap()
        .set_pulls_from_refineries(ResourceKind::Planks, true);
    let worker = spawn_worker(&mut world, CellCoord::new(1, 2));

    manage_building_logistics(&mut world);

    assert!(world.get::<AiBuildingHaul>(worker).is_none());
}

#[test]
fn deactivation_cancels_a_loaded_haul_and_preserves_it_for_recovery() {
    let mut world = navigation_world();
    let storage = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(2, 2));
    assert!(world
        .get_mut::<StorageInventory>(storage)
        .unwrap()
        .add(ResourceKind::Wood, 30));
    let refinery = spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(6, 2));
    world
        .get_mut::<RefineryPullConfig>(refinery)
        .unwrap()
        .set_pulls_from_storage(ResourceKind::Wood, true);
    let worker = spawn_worker(&mut world, CellCoord::new(1, 2));

    manage_building_logistics(&mut world);
    manage_building_logistics(&mut world);
    assert!(world.get::<Wheelbarrow>(worker).unwrap().stack().is_some());

    cancel_work_involving_building(&mut world, refinery);

    assert!(world.get::<AiBuildingHaul>(worker).is_none());
    assert!(world.get::<AiWheelbarrowRecovery>(worker).is_some());
    assert_eq!(
        world
            .get::<Wheelbarrow>(worker)
            .unwrap()
            .stack()
            .unwrap()
            .amount(),
        25
    );
    assert!(world.resource::<ReservationLedger>().claims().is_empty());

    manage_wheelbarrow_recovery(&mut world);
    assert!(world.get::<AiWheelbarrowRecovery>(worker).is_none());
    assert!(world.get::<Wheelbarrow>(worker).is_none());
    assert_eq!(
        world
            .get::<StorageInventory>(storage)
            .unwrap()
            .contents()
            .get(ResourceKind::Wood),
        30
    );
}

#[test]
fn loaded_wheelbarrow_counts_as_owned_surface_stock() {
    let mut world = World::new();
    world.spawn(Wheelbarrow::of(ResourceKind::Stone, 17));

    assert_eq!(
        resource_overview(&mut world)
            .usable()
            .get(ResourceKind::Stone),
        17
    );
}

#[test]
fn mixed_input_reservations_share_the_refinery_total_capacity() {
    let mut world = navigation_world();
    let crops = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(2, 2));
    let berries = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(2, 4));
    assert!(world
        .get_mut::<StorageInventory>(crops)
        .unwrap()
        .add(ResourceKind::Crops, 100));
    assert!(world
        .get_mut::<StorageInventory>(berries)
        .unwrap()
        .add(ResourceKind::WildBerries, 100));
    let kitchen = spawn_refinery(&mut world, BuildingKind::Kitchen, CellCoord::new(7, 2));
    {
        let mut pull = world.get_mut::<RefineryPullConfig>(kitchen).unwrap();
        pull.set_pulls_from_storage(ResourceKind::Crops, true);
        pull.set_pulls_from_storage(ResourceKind::WildBerries, true);
    }
    let workers = (0..8)
        .map(|_| spawn_worker(&mut world, CellCoord::new(1, 2)))
        .collect::<Vec<_>>();

    manage_building_logistics(&mut world);

    assert_eq!(
        workers
            .iter()
            .filter(|worker| world.get::<AiBuildingHaul>(**worker).is_some())
            .count(),
        4
    );
    assert_eq!(
        world
            .resource::<ReservationLedger>()
            .reserved_capacity_to(game_engine::refining::SinkEndpoint::RefineryInput(kitchen)),
        100
    );
}

#[test]
fn building_logistics_does_not_replace_construction_labor_or_its_route() {
    let mut world = navigation_world();
    let source = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(2, 2));
    assert!(world
        .get_mut::<StorageInventory>(source)
        .unwrap()
        .add(ResourceKind::Wood, 30));
    let refinery = spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(6, 2));
    world
        .get_mut::<RefineryPullConfig>(refinery)
        .unwrap()
        .set_pulls_from_storage(ResourceKind::Wood, true);
    let kind = BuildingKind::Depot;
    let cost = kind.definition().construction_cost();
    let labor_site = world
        .spawn((
            BuildingBlueprint {
                kind,
                footprint: BuildingFootprint::new(CellCoord::new(5, 5), 1, 1),
            },
            ConstructionProgress::new(cost).with_required_labor(10),
        ))
        .id();
    let worker = spawn_worker(&mut world, CellCoord::new(1, 5));
    refresh_navigation_snapshot(&mut world);

    manage_construction_labor(&mut world);
    let labor = *world.get::<AiConstructionLabor>(worker).unwrap();
    assert_eq!(labor.site(), labor_site);
    let labor_route = world.get::<NpcRoute>(worker).unwrap().goals().to_vec();

    manage_building_logistics(&mut world);

    assert!(world.get::<AiBuildingHaul>(worker).is_none());
    assert_eq!(world.get::<NpcRoute>(worker).unwrap().goals(), labor_route);
    assert!(world.resource::<ReservationLedger>().claims().is_empty());

    world.get_mut::<NpcPosition>(worker).unwrap().coord = labor.interaction_cell();
    for _ in 0..3 {
        manage_construction_labor(&mut world);
        manage_building_logistics(&mut world);
    }

    assert_eq!(
        world
            .get::<ConstructionProgress>(labor_site)
            .unwrap()
            .labor_completed(),
        3
    );
    assert!(world.get::<AiBuildingHaul>(worker).is_none());
}

#[test]
fn construction_labor_does_not_preempt_an_existing_building_haul() {
    let mut world = navigation_world();
    let source = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(2, 2));
    assert!(world
        .get_mut::<StorageInventory>(source)
        .unwrap()
        .add(ResourceKind::Wood, 30));
    let refinery = spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(6, 2));
    world
        .get_mut::<RefineryPullConfig>(refinery)
        .unwrap()
        .set_pulls_from_storage(ResourceKind::Wood, true);
    let worker = spawn_worker(&mut world, CellCoord::new(1, 2));
    manage_building_logistics(&mut world);
    let original_haul = *world.get::<AiBuildingHaul>(worker).unwrap();
    let original_claims = world.resource::<ReservationLedger>().claims().to_vec();

    let kind = BuildingKind::Depot;
    let cost = kind.definition().construction_cost();
    world.spawn((
        BuildingBlueprint {
            kind,
            footprint: BuildingFootprint::new(CellCoord::new(5, 5), 1, 1),
        },
        ConstructionProgress::new(cost).with_required_labor(10),
    ));
    refresh_navigation_snapshot(&mut world);

    manage_construction_labor(&mut world);

    assert_eq!(world.get::<AiBuildingHaul>(worker), Some(&original_haul));
    assert!(world.get::<AiConstructionLabor>(worker).is_none());
    assert!(world.get::<AiConstructBuilding>(worker).is_none());
    assert_eq!(
        world.resource::<ReservationLedger>().claims(),
        original_claims.as_slice()
    );
}

fn spawn_storage(world: &mut World, kind: BuildingKind, coord: CellCoord) -> Entity {
    world
        .spawn((
            Building::new(kind, BuildingFootprint::new(coord, 1, 1)),
            StorageInventory::for_kind(kind),
            BuildingActivity::active(),
            StoragePullConfig::default(),
        ))
        .id()
}

fn spawn_refinery(world: &mut World, kind: BuildingKind, coord: CellCoord) -> Entity {
    world
        .spawn((
            Building::new(kind, BuildingFootprint::new(coord, 1, 1)),
            RefineryInventory::empty(),
            RefineryPullConfig::default(),
            BuildingActivity::active(),
        ))
        .id()
}

fn spawn_worker(world: &mut World, coord: CellCoord) -> Entity {
    world
        .spawn((Npc, NpcPosition::new(coord), CarriedResource::empty()))
        .id()
}

fn navigation_world() -> World {
    let size = GridSize::new(10, 8);
    let mut world = World::new();
    world.insert_resource(Grid::new(size.width(), size.height()));
    world.insert_resource(ReservationLedger::default());
    let mut index = TileIndex::new(size);
    for coord in size.iter_coords() {
        let tile = world.spawn(TileBundle::new(coord)).id();
        assert!(index.set(coord, tile));
    }
    world.insert_resource(index);
    world
}
