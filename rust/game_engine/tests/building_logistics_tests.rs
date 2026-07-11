use bevy_ecs::prelude::*;
use game_engine::buildings::{
    Building, BuildingActivity, BuildingBlueprint, BuildingFootprint, BuildingKind,
    ConstructionProgress, RefineryPullConfig, StorageInventory, StoragePullConfig,
};
use game_engine::components::{
    AiConstructBuilding, CarriedResource, Npc, NpcPosition, ResourceNode, TilePosition, Wheelbarrow,
};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::logistics::{
    cancel_work_involving_building, manage_building_logistics, manage_construction_logistics,
    manage_wheelbarrow_recovery, AiBuildingHaul, AiConstructionHaul, AiWheelbarrowRecovery,
    BuildingHaulPhase,
};
use game_engine::navigation::{refresh_navigation_snapshot, NpcRoute};
use game_engine::refining::{RefineryInventory, ReservationLedger, SinkEndpoint, StockEndpoint};
use game_engine::resources::{resource_overview, ResourceAmounts, ResourceKind};
use game_engine::roads::{RoadBlueprint, RoadTier};
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
fn construction_materials_preempt_unloaded_refinery_supply_and_reuse_its_stock() {
    let mut world = navigation_world();
    let storage = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(2, 2));
    assert!(world
        .get_mut::<StorageInventory>(storage)
        .unwrap()
        .add(ResourceKind::Wood, 20));
    let refinery = spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(7, 2));
    world
        .get_mut::<RefineryPullConfig>(refinery)
        .unwrap()
        .set_pulls_from_storage(ResourceKind::Wood, true);
    let worker = spawn_worker(&mut world, CellCoord::new(1, 2));

    manage_building_logistics(&mut world);
    assert_eq!(
        world.get::<AiBuildingHaul>(worker).unwrap().phase(),
        BuildingHaulPhase::ToSource
    );
    assert_eq!(
        world
            .resource::<ReservationLedger>()
            .reserved_from(StockEndpoint::Warehouse(storage), ResourceKind::Wood),
        20
    );

    let blueprint = world
        .spawn((
            BuildingBlueprint {
                kind: BuildingKind::Depot,
                footprint: BuildingFootprint::new(CellCoord::new(5, 5), 1, 1),
            },
            ConstructionProgress::new(ResourceAmounts::zero()),
        ))
        .id();
    manage_construction_logistics(&mut world);

    let haul = *world.get::<AiConstructionHaul>(worker).unwrap();
    assert_eq!(haul.blueprint(), blueprint);
    assert_eq!(haul.source(), Some(StockEndpoint::Warehouse(storage)));
    assert_eq!(haul.amount(), 20);
    assert!(world.get::<AiBuildingHaul>(worker).is_none());
    let claims = world.resource::<ReservationLedger>().claims();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].worker, worker);
    assert_eq!(claims[0].sink, SinkEndpoint::Blueprint(blueprint));
    assert_eq!(claims[0].source, Some(StockEndpoint::Warehouse(storage)));
}

#[test]
fn construction_preempts_in_progress_natural_gathering_without_consuming_resource() {
    let mut world = navigation_world();
    let node = spawn_resource_node(&mut world, CellCoord::new(2, 2), ResourceKind::Wood, 1);
    spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(7, 2));
    let worker = spawn_worker(&mut world, CellCoord::new(2, 1));

    manage_building_logistics(&mut world);
    manage_building_logistics(&mut world);
    assert_eq!(
        world.get::<AiBuildingHaul>(worker).unwrap().phase(),
        BuildingHaulPhase::Gathering { progress_ticks: 0 }
    );

    let blueprint = world
        .spawn((
            BuildingBlueprint {
                kind: BuildingKind::Depot,
                footprint: BuildingFootprint::new(CellCoord::new(5, 5), 1, 1),
            },
            ConstructionProgress::new(ResourceAmounts::zero()),
        ))
        .id();
    manage_construction_logistics(&mut world);

    let haul = *world.get::<AiConstructionHaul>(worker).unwrap();
    assert_eq!(haul.blueprint(), blueprint);
    assert_eq!(haul.source(), Some(StockEndpoint::NaturalNode(node)));
    assert!(world.get::<AiBuildingHaul>(worker).is_none());
    assert_eq!(world.get::<ResourceNode>(node).unwrap().quantity, 1);
    assert_eq!(world.resource::<ReservationLedger>().claims().len(), 1);
    assert_eq!(
        world.resource::<ReservationLedger>().claims()[0].sink,
        SinkEndpoint::Blueprint(blueprint)
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
fn road_labor_preempts_unloaded_refinery_supply() {
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
    assert_eq!(
        world.get::<AiBuildingHaul>(worker).unwrap().phase(),
        BuildingHaulPhase::ToSource
    );
    assert_eq!(world.get::<Wheelbarrow>(worker).unwrap().stack(), None);

    let road = world
        .spawn((
            RoadBlueprint {
                coord: CellCoord::new(5, 5),
                target_tier: RoadTier::DirtPath,
            },
            ConstructionProgress::new(ResourceAmounts::zero()).with_required_labor(10),
        ))
        .id();

    manage_construction_labor(&mut world);

    assert!(world.get::<AiBuildingHaul>(worker).is_none());
    assert!(world.get::<Wheelbarrow>(worker).is_none());
    assert_eq!(
        world.get::<AiConstructionLabor>(worker).unwrap().site(),
        road
    );
    assert_eq!(
        world
            .get::<AiConstructBuilding>(worker)
            .unwrap()
            .blueprint(),
        road
    );
    assert!(world.resource::<ReservationLedger>().claims().is_empty());
}

#[test]
fn loaded_refinery_supply_finishes_before_road_labor_starts() {
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
    manage_building_logistics(&mut world);
    let loaded_haul = *world.get::<AiBuildingHaul>(worker).unwrap();
    assert_eq!(loaded_haul.phase(), BuildingHaulPhase::ToSink);
    assert!(world.get::<Wheelbarrow>(worker).unwrap().stack().is_some());

    let road = world
        .spawn((
            RoadBlueprint {
                coord: CellCoord::new(5, 5),
                target_tier: RoadTier::DirtPath,
            },
            ConstructionProgress::new(ResourceAmounts::zero()).with_required_labor(10),
        ))
        .id();

    manage_construction_labor(&mut world);

    assert_eq!(world.get::<AiBuildingHaul>(worker), Some(&loaded_haul));
    assert!(world.get::<AiConstructionLabor>(worker).is_none());

    world.get_mut::<NpcPosition>(worker).unwrap().coord = CellCoord::new(5, 2);
    manage_building_logistics(&mut world);
    manage_construction_labor(&mut world);

    assert!(world.get::<AiBuildingHaul>(worker).is_none());
    assert_eq!(
        world.get::<AiConstructionLabor>(worker).unwrap().site(),
        road
    );
}

#[test]
fn road_labor_does_not_preempt_refinery_output_storage_haul() {
    let mut world = navigation_world();
    let refinery = spawn_refinery(&mut world, BuildingKind::Sawmill, CellCoord::new(2, 2));
    assert!(world
        .get_mut::<RefineryInventory>(refinery)
        .unwrap()
        .add_output(BuildingKind::Sawmill, ResourceKind::Planks, 25));
    let storage = spawn_storage(&mut world, BuildingKind::Depot, CellCoord::new(6, 2));
    world
        .get_mut::<StoragePullConfig>(storage)
        .unwrap()
        .set_pulls_from_refineries(ResourceKind::Planks, true);
    let worker = spawn_worker(&mut world, CellCoord::new(1, 2));
    manage_building_logistics(&mut world);
    let original_haul = *world.get::<AiBuildingHaul>(worker).unwrap();

    world.spawn((
        RoadBlueprint {
            coord: CellCoord::new(5, 5),
            target_tier: RoadTier::DirtPath,
        },
        ConstructionProgress::new(ResourceAmounts::zero()).with_required_labor(10),
    ));

    manage_construction_labor(&mut world);

    assert_eq!(world.get::<AiBuildingHaul>(worker), Some(&original_haul));
    assert!(world.get::<AiConstructionLabor>(worker).is_none());
    assert!(world.get::<AiConstructBuilding>(worker).is_none());
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

fn spawn_resource_node(
    world: &mut World,
    coord: CellCoord,
    kind: ResourceKind,
    quantity: u32,
) -> Entity {
    let entity = world.resource::<TileIndex>().get(coord).unwrap();
    world
        .entity_mut(entity)
        .insert((TilePosition { coord }, ResourceNode { kind, quantity }));
    entity
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
