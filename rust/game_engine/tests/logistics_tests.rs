use bevy_ecs::prelude::*;
use game_engine::ai::{AiKeepEnoughFoodInInventory, AiSearchForFood};
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingFootprint, BuildingKind, ConstructionProgress,
    WarehouseInventory,
};
use game_engine::components::{Npc, NpcInventory, NpcPosition};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::logistics::{manage_construction_logistics, manage_food_logistics};
use game_engine::refining::{RefineryInventory, RefineryProduction, ReservationLedger};
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

    assert_eq!(
        world
            .get::<NpcInventory>(npc)
            .unwrap()
            .contents()
            .get(ResourceKind::Food),
        20
    );
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

    assert_eq!(
        world
            .get::<NpcInventory>(npc)
            .unwrap()
            .contents()
            .get(ResourceKind::Food),
        20
    );
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
            NpcInventory::default(),
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
            .get::<NpcInventory>(npc)
            .unwrap()
            .contents()
            .get(ResourceKind::Planks),
        0
    );
}

fn hungry_npc(world: &mut World, coord: CellCoord) -> Entity {
    world
        .spawn((
            Npc,
            NpcPosition::new(coord),
            NpcInventory::new(ResourceAmounts::of(ResourceKind::Food, 5)),
            AiKeepEnoughFoodInInventory::new(5, 20),
        ))
        .id()
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
