use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::ai::RESOURCE_GATHER_TICKS_PER_UNIT;
use game_engine::ai::{AiKeepEnoughFoodInInventory, AiSearchForFood};
use game_engine::buildings::{
    system_complete_building_construction, Building, BuildingBlueprint, BuildingFootprint,
    BuildingKind, ConstructionProgress, WarehouseInventory,
};
use game_engine::components::{
    CarriedResource, FoodPouch, Npc, NpcPosition, ResourceNode, Terrain, TerrainKind, TilePosition,
};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::logistics::{
    manage_construction_logistics, manage_food_logistics, AiConstructionHaul, AiFoodHaul,
};
use game_engine::refining::{
    RefineryInventory, RefineryProduction, Reservation, ReservationLedger, SinkEndpoint,
    StockEndpoint,
};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::tile::{TileBundle, TileIndex};

#[test]
fn hungry_npc_withdraws_only_cooked_food_from_a_warehouse() {
    let mut world = navigation_world();
    let warehouse = world
        .spawn((
            Building::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(3, 3), 2, 2),
            ),
            WarehouseInventory::empty(),
        ))
        .id();
    {
        let mut inventory = world.get_mut::<WarehouseInventory>(warehouse).unwrap();
        assert!(inventory.add(ResourceKind::Food, 20));
        assert!(inventory.add(ResourceKind::Crops, 30));
        assert!(inventory.add(ResourceKind::WildBerries, 30));
    }
    let npc = hungry_npc(&mut world, CellCoord::new(2, 3));

    manage_food_logistics(&mut world);

    assert_eq!(world.get::<FoodPouch>(npc).unwrap().amount(), 20);
    let warehouse = world.get::<WarehouseInventory>(warehouse).unwrap();
    assert_eq!(warehouse.contents().get(ResourceKind::Food), 5);
    assert_eq!(warehouse.contents().get(ResourceKind::Crops), 30);
    assert_eq!(warehouse.contents().get(ResourceKind::WildBerries), 30);
    assert!(world.get::<AiSearchForFood>(npc).is_none());
}

#[test]
fn hungry_npc_can_withdraw_food_from_kitchen_output() {
    let mut world = navigation_world();
    let kitchen = world
        .spawn((
            Building::new(
                BuildingKind::Kitchen,
                BuildingFootprint::new(CellCoord::new(3, 3), 2, 2),
            ),
            RefineryInventory::empty(),
            RefineryProduction::default(),
        ))
        .id();
    assert!(world
        .get_mut::<RefineryInventory>(kitchen)
        .unwrap()
        .add_output(BuildingKind::Kitchen, ResourceKind::Food, 20));
    let npc = hungry_npc(&mut world, CellCoord::new(2, 3));

    manage_food_logistics(&mut world);

    assert_eq!(world.get::<FoodPouch>(npc).unwrap().amount(), 20);
    assert_eq!(
        world
            .get::<RefineryInventory>(kitchen)
            .unwrap()
            .output_contents()
            .get(ResourceKind::Food),
        5
    );
}

#[test]
fn equidistant_food_sources_use_the_lower_entity_id() {
    let mut world = navigation_world();
    let first = spawn_food_warehouse(&mut world, CellCoord::new(1, 1), 10);
    let second = spawn_food_warehouse(&mut world, CellCoord::new(5, 1), 10);
    let npc = hungry_npc(&mut world, CellCoord::new(3, 6));

    manage_food_logistics(&mut world);

    let expected = [first, second]
        .into_iter()
        .min_by_key(|entity| entity.to_bits())
        .unwrap();
    assert_eq!(
        world.get::<AiFoodHaul>(npc).unwrap().source(),
        StockEndpoint::Warehouse(expected)
    );
}

#[test]
fn food_source_selection_skips_fully_reserved_nearer_stock() {
    let mut world = navigation_world();
    let nearer = spawn_food_warehouse(&mut world, CellCoord::new(3, 3), 10);
    let farther = spawn_food_warehouse(&mut world, CellCoord::new(0, 0), 10);
    let reserving_worker = world.spawn_empty().id();
    let task = world.spawn_empty().id();
    assert!(world
        .resource_mut::<ReservationLedger>()
        .claim(Reservation {
            worker: reserving_worker,
            source: Some(StockEndpoint::Warehouse(nearer)),
            sink: SinkEndpoint::FoodPouch(reserving_worker),
            kind: ResourceKind::Food,
            amount: 10,
            task,
        }));
    let npc = hungry_npc(&mut world, CellCoord::new(3, 7));

    manage_food_logistics(&mut world);

    assert_eq!(
        world.get::<AiFoodHaul>(npc).unwrap().source(),
        StockEndpoint::Warehouse(farther)
    );
}

#[test]
fn food_source_selection_skips_an_unreachable_nearer_inventory() {
    let mut world = navigation_world();
    let unreachable = spawn_food_warehouse(&mut world, CellCoord::new(3, 3), 10);
    for coord in [
        CellCoord::new(2, 3),
        CellCoord::new(4, 3),
        CellCoord::new(3, 2),
        CellCoord::new(3, 4),
    ] {
        set_terrain(&mut world, coord, TerrainKind::Water);
    }
    let reachable = spawn_food_warehouse(&mut world, CellCoord::new(0, 0), 10);
    let npc = hungry_npc(&mut world, CellCoord::new(3, 7));

    manage_food_logistics(&mut world);

    assert_ne!(unreachable, reachable);
    assert_eq!(
        world.get::<AiFoodHaul>(npc).unwrap().source(),
        StockEndpoint::Warehouse(reachable)
    );
}

#[test]
fn construction_withdraws_refined_material_from_owned_inventory() {
    let mut world = navigation_world();
    let warehouse = world
        .spawn((
            Building::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(1, 1), 2, 2),
            ),
            WarehouseInventory::empty(),
        ))
        .id();
    assert!(world
        .get_mut::<WarehouseInventory>(warehouse)
        .unwrap()
        .add(ResourceKind::Planks, 10));
    let blueprint = world
        .spawn((
            BuildingBlueprint {
                kind: BuildingKind::SmallHouse,
                footprint: BuildingFootprint::new(CellCoord::new(5, 1), 1, 1),
            },
            ConstructionProgress::new(ResourceAmounts::zero()),
        ))
        .id();
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(0, 1)),
            CarriedResource::default(),
        ))
        .id();

    manage_construction_logistics(&mut world); // claim at warehouse
    manage_construction_logistics(&mut world); // withdraw
    world.get_mut::<NpcPosition>(npc).unwrap().coord = CellCoord::new(4, 1);
    manage_construction_logistics(&mut world); // deposit at blueprint

    assert_eq!(
        world
            .get::<ConstructionProgress>(blueprint)
            .unwrap()
            .deposited()
            .get(ResourceKind::Planks),
        10
    );
    assert_eq!(
        world
            .get::<WarehouseInventory>(warehouse)
            .unwrap()
            .contents()
            .get(ResourceKind::Planks),
        0
    );
    assert_eq!(
        world
            .get::<CarriedResource>(npc)
            .unwrap()
            .contents()
            .get(ResourceKind::Planks),
        0
    );
}

#[test]
fn natural_construction_reservations_match_one_unit_hauls_for_all_available_npcs() {
    let mut world = navigation_world();
    let blueprint = spawn_sawmill_blueprint(&mut world, CellCoord::new(5, 5));
    let wood = spawn_resource_node(&mut world, CellCoord::new(3, 3), ResourceKind::Wood, 20);
    let stone = spawn_resource_node(&mut world, CellCoord::new(4, 3), ResourceKind::Stone, 10);
    let npcs = (0..5)
        .map(|x| spawn_available_npc(&mut world, CellCoord::new(x, 7)))
        .collect::<Vec<_>>();

    manage_construction_logistics(&mut world);

    let hauls = npcs
        .iter()
        .map(|npc| {
            world
                .get::<AiConstructionHaul>(*npc)
                .copied()
                .expect("every available NPC should receive construction work")
        })
        .collect::<Vec<_>>();
    assert!(hauls.iter().all(|haul| {
        haul.blueprint() == blueprint
            && haul.amount() == 1
            && matches!(haul.source(), Some(StockEndpoint::NaturalNode(_)))
    }));

    let ledger = world.resource::<ReservationLedger>();
    assert_eq!(ledger.claims().len(), 5);
    assert!(ledger.claims().iter().all(|claim| claim.amount == 1));
    assert_eq!(
        ledger.reserved_to(SinkEndpoint::Blueprint(blueprint), ResourceKind::Wood)
            + ledger.reserved_to(SinkEndpoint::Blueprint(blueprint), ResourceKind::Stone),
        5
    );
    assert!(ledger.reserved_from(StockEndpoint::NaturalNode(wood), ResourceKind::Wood) <= 20);
    assert!(ledger.reserved_from(StockEndpoint::NaturalNode(stone), ResourceKind::Stone) <= 10);
}

#[test]
fn natural_construction_does_not_reserve_more_workers_than_source_stock() {
    let mut world = navigation_world();
    let blueprint = spawn_sawmill_blueprint(&mut world, CellCoord::new(5, 5));
    world
        .get_mut::<ConstructionProgress>(blueprint)
        .unwrap()
        .deposit(
            ResourceKind::Stone,
            10,
            BuildingKind::Sawmill.definition().construction_cost(),
        );
    let wood = spawn_resource_node(&mut world, CellCoord::new(3, 3), ResourceKind::Wood, 2);
    let npcs = (0..5)
        .map(|x| spawn_available_npc(&mut world, CellCoord::new(x, 7)))
        .collect::<Vec<_>>();

    manage_construction_logistics(&mut world);

    assert_eq!(
        npcs.iter()
            .filter(|npc| world.get::<AiConstructionHaul>(**npc).is_some())
            .count(),
        2
    );
    let ledger = world.resource::<ReservationLedger>();
    assert_eq!(
        ledger.reserved_from(StockEndpoint::NaturalNode(wood), ResourceKind::Wood),
        2
    );
    assert_eq!(
        ledger.reserved_to(SinkEndpoint::Blueprint(blueprint), ResourceKind::Wood),
        2
    );
}

#[test]
fn concurrent_natural_construction_deposits_exact_claims_and_completes_blueprint() {
    let mut world = navigation_world();
    let blueprint = spawn_sawmill_blueprint(&mut world, CellCoord::new(5, 5));
    let cost = BuildingKind::Sawmill.definition().construction_cost();
    {
        let mut progress = world.get_mut::<ConstructionProgress>(blueprint).unwrap();
        progress.deposit(ResourceKind::Wood, 18, cost);
        progress.deposit(ResourceKind::Stone, 10, cost);
    }
    let wood = spawn_resource_node(&mut world, CellCoord::new(3, 3), ResourceKind::Wood, 2);
    let npcs = [
        spawn_available_npc(&mut world, CellCoord::new(3, 2)),
        spawn_available_npc(&mut world, CellCoord::new(3, 2)),
    ];

    manage_construction_logistics(&mut world); // assign both one-unit claims
    manage_construction_logistics(&mut world); // enter gathering
    for _ in 0..RESOURCE_GATHER_TICKS_PER_UNIT {
        manage_construction_logistics(&mut world);
    }

    assert!(world.get::<ResourceNode>(wood).is_none());
    for npc in npcs {
        world.get_mut::<NpcPosition>(npc).unwrap().coord = CellCoord::new(4, 5);
    }
    manage_construction_logistics(&mut world);

    assert!(world
        .get::<ConstructionProgress>(blueprint)
        .unwrap()
        .is_complete(cost));
    assert!(world.resource::<ReservationLedger>().claims().is_empty());
    assert!(npcs
        .iter()
        .all(|npc| world.get::<AiConstructionHaul>(*npc).is_none()));

    world
        .run_system_once(system_complete_building_construction)
        .expect("building completion should run");
    assert_eq!(
        world
            .get::<Building>(blueprint)
            .map(|building| building.kind),
        Some(BuildingKind::Sawmill)
    );
}

fn hungry_npc(world: &mut World, coord: CellCoord) -> Entity {
    world
        .spawn((
            Npc,
            NpcPosition::new(coord),
            FoodPouch::new(5),
            CarriedResource::empty(),
            AiKeepEnoughFoodInInventory::new(5, 20),
        ))
        .id()
}

fn spawn_food_warehouse(world: &mut World, coord: CellCoord, amount: u32) -> Entity {
    let entity = world
        .spawn((
            Building::new(BuildingKind::Warehouse, BuildingFootprint::new(coord, 1, 1)),
            WarehouseInventory::empty(),
        ))
        .id();
    assert!(world
        .get_mut::<WarehouseInventory>(entity)
        .unwrap()
        .add(ResourceKind::Food, amount));
    entity
}

fn spawn_sawmill_blueprint(world: &mut World, coord: CellCoord) -> Entity {
    world
        .spawn((
            BuildingBlueprint {
                kind: BuildingKind::Sawmill,
                footprint: BuildingFootprint::new(coord, 2, 2),
            },
            ConstructionProgress::new(ResourceAmounts::zero()),
        ))
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

fn spawn_available_npc(world: &mut World, coord: CellCoord) -> Entity {
    world
        .spawn((Npc, NpcPosition::new(coord), CarriedResource::default()))
        .id()
}

fn set_terrain(world: &mut World, coord: CellCoord, kind: TerrainKind) {
    let tile = world.resource::<TileIndex>().get(coord).unwrap();
    world.get_mut::<Terrain>(tile).unwrap().kind = kind;
}

fn navigation_world() -> World {
    let size = GridSize::new(8, 8);
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
