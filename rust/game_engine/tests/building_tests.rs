use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::buildings::{
    place_building_blueprint, system_complete_building_construction, Building, BuildingBlueprint,
    BuildingFootprint, BuildingKind, BuildingPlacementError, ConstructionProgress,
    WarehouseInventory, DEFAULT_WAREHOUSE_INVENTORY_MAX_SIZE,
};
use game_engine::components::{Terrain, TerrainKind};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::housing::House;
use game_engine::npcs::{Npc, NpcPosition};
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::simulation::GameSimulation;

#[test]
fn test_building_definitions_include_dimensions_and_costs() {
    let warehouse = BuildingKind::Warehouse.definition();
    assert_eq!(warehouse.kind(), BuildingKind::Warehouse);
    assert_eq!(warehouse.width(), 2);
    assert_eq!(warehouse.height(), 2);
    assert_eq!(warehouse.construction_cost().get(ResourceKind::Planks), 40);
    assert_eq!(
        warehouse.construction_cost().get(ResourceKind::StoneBlocks),
        20
    );
    assert_eq!(warehouse.construction_cost().get(ResourceKind::Food), 0);
    assert_eq!(warehouse.construction_cost().get(ResourceKind::Gold), 0);

    let town_hall = BuildingKind::TownHall.definition();
    assert_eq!(town_hall.kind(), BuildingKind::TownHall);
    assert_eq!(town_hall.width(), 3);
    assert_eq!(town_hall.height(), 3);
    assert_eq!(town_hall.construction_cost().get(ResourceKind::Planks), 80);
    assert_eq!(
        town_hall.construction_cost().get(ResourceKind::StoneBlocks),
        60
    );
    assert_eq!(town_hall.construction_cost().get(ResourceKind::Food), 0);
    assert_eq!(town_hall.construction_cost().get(ResourceKind::Gold), 20);

    for (kind, stone_cost) in [(BuildingKind::Sawmill, 10), (BuildingKind::Stoneworks, 20)] {
        let definition = kind.definition();
        assert_eq!((definition.width(), definition.height()), (2, 2));
        assert_eq!(definition.construction_cost().get(ResourceKind::Wood), 20);
        assert_eq!(
            definition.construction_cost().get(ResourceKind::Stone),
            stone_cost
        );
    }
    let kitchen = BuildingKind::Kitchen.definition();
    assert_eq!((kitchen.width(), kitchen.height()), (2, 2));
    assert_eq!(kitchen.construction_cost().get(ResourceKind::Planks), 20);
    assert_eq!(
        kitchen.construction_cost().get(ResourceKind::StoneBlocks),
        10
    );

    let farm = BuildingKind::Farm.definition();
    assert_eq!(farm.kind(), BuildingKind::Farm);
    assert_eq!(farm.width(), 3);
    assert_eq!(farm.height(), 3);
    assert_eq!(farm.construction_cost().get(ResourceKind::Planks), 20);
    assert_eq!(farm.construction_cost().get(ResourceKind::StoneBlocks), 30);
    assert_eq!(farm.construction_cost().get(ResourceKind::Food), 0);
    assert_eq!(farm.construction_cost().get(ResourceKind::Gold), 0);

    let field = BuildingKind::Field.definition();
    assert_eq!(field.kind(), BuildingKind::Field);
    assert_eq!(field.width(), 1);
    assert_eq!(field.height(), 1);
    assert_eq!(field.construction_cost().get(ResourceKind::Planks), 5);
    assert_eq!(field.construction_cost().get(ResourceKind::StoneBlocks), 1);
    assert_eq!(field.construction_cost().get(ResourceKind::Food), 0);
    assert_eq!(field.construction_cost().get(ResourceKind::Gold), 0);

    for (kind, width, height, cost, capacity) in [
        (
            BuildingKind::SmallHouse,
            1,
            1,
            ResourceAmounts::zero()
                .with(ResourceKind::Planks, 10)
                .with(ResourceKind::StoneBlocks, 5),
            2,
        ),
        (
            BuildingKind::MediumHouse,
            2,
            2,
            ResourceAmounts::zero()
                .with(ResourceKind::Planks, 30)
                .with(ResourceKind::StoneBlocks, 15),
            4,
        ),
        (
            BuildingKind::LargeHouse,
            3,
            3,
            ResourceAmounts::zero()
                .with(ResourceKind::Planks, 60)
                .with(ResourceKind::StoneBlocks, 30),
            8,
        ),
    ] {
        let definition = kind.definition();
        assert_eq!(definition.kind(), kind);
        assert_eq!(definition.width(), width);
        assert_eq!(definition.height(), height);
        assert_eq!(definition.construction_cost(), cost);
        assert_eq!(definition.housing_capacity(), Some(capacity));
    }
    for kind in [
        BuildingKind::Warehouse,
        BuildingKind::TownHall,
        BuildingKind::Sawmill,
        BuildingKind::Stoneworks,
        BuildingKind::Kitchen,
        BuildingKind::Farm,
        BuildingKind::Field,
    ] {
        assert_eq!(kind.definition().housing_capacity(), None);
    }
}

#[test]
fn test_place_building_blueprint_inside_bounds() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));

    let entity = simulation
        .place_building_blueprint(surface, BuildingKind::Warehouse, CellCoord::new(1, 1))
        .expect("warehouse should place inside bounds");

    let info = simulation
        .with_surface_world(surface, |world| {
            let blueprint = world.get::<BuildingBlueprint>(entity)?;
            let progress = world.get::<ConstructionProgress>(entity)?;
            Some((blueprint.kind, blueprint.footprint, progress.deposited()))
        })
        .expect("placed building should be queryable");

    assert_eq!(info.0, BuildingKind::Warehouse);
    assert_eq!(info.1.origin(), CellCoord::new(1, 1));
    assert_eq!(info.1.width(), 2);
    assert_eq!(info.1.height(), 2);
    for kind in ResourceKind::ALL {
        assert_eq!(info.2.get(kind), 0);
    }
}

#[test]
fn test_place_building_blueprint_rejects_out_of_bounds() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));

    let result =
        simulation.place_building_blueprint(surface, BuildingKind::TownHall, CellCoord::new(2, 2));

    assert_eq!(result, Err(BuildingPlacementError::OutOfBounds));
}

#[test]
fn test_place_building_blueprint_rejects_blueprint_overlap() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(8, 8));

    simulation
        .place_building_blueprint(surface, BuildingKind::Warehouse, CellCoord::new(2, 2))
        .expect("first blueprint should place");
    let result =
        simulation.place_building_blueprint(surface, BuildingKind::TownHall, CellCoord::new(3, 3));

    assert_eq!(result, Err(BuildingPlacementError::OverlapsBuilding));
}

#[test]
fn test_standalone_field_blueprint_requires_farm_owner() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));

    let result =
        simulation.place_building_blueprint(surface, BuildingKind::Field, CellCoord::new(1, 1));

    assert_eq!(result, Err(BuildingPlacementError::FieldRequiresFarm));
}

#[test]
fn test_blueprint_can_overlap_npc() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();
    let npc_coord = simulation
        .with_surface_world(surface, first_npc_coord)
        .expect("default surface should have an NPC");

    let result = simulation.place_building_blueprint(surface, BuildingKind::Warehouse, npc_coord);

    assert!(result.is_ok());
}

#[test]
fn test_blueprint_rejects_resource_node_overlap() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();
    let size = simulation.grid_size(surface);
    let resource_coord = simulation
        .with_surface_world(surface, first_buildable_resource_node_coord)
        .expect("default surface should have buildable resource nodes");
    let origin = origin_for_footprint_containing(size, resource_coord, 2, 2);
    let footprint = BuildingFootprint::new(origin, 2, 2);

    assert!(footprint.contains(resource_coord));
    let result = simulation.place_building_blueprint(surface, BuildingKind::Warehouse, origin);

    assert_eq!(result, Err(BuildingPlacementError::BlockedByResourceNode));
}

#[test]
fn test_construction_progress_starts_empty() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let entity = simulation
        .place_building_blueprint(surface, BuildingKind::TownHall, CellCoord::new(0, 0))
        .expect("town hall should place");

    let deposited = simulation
        .with_surface_world(surface, |world| {
            world
                .get::<ConstructionProgress>(entity)
                .map(|p| p.deposited())
        })
        .expect("construction progress should exist");

    for kind in ResourceKind::ALL {
        assert_eq!(deposited.get(kind), 0);
    }
}

#[test]
fn test_warehouse_blueprint_does_not_have_inventory() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let entity = simulation
        .place_building_blueprint(surface, BuildingKind::Warehouse, CellCoord::new(0, 0))
        .expect("warehouse should place");

    let has_inventory = simulation.with_surface_world(surface, |world| {
        world.get::<WarehouseInventory>(entity).is_some()
    });

    assert!(!has_inventory);
}

#[test]
fn test_town_hall_does_not_have_warehouse_inventory() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let entity = simulation
        .place_building_blueprint(surface, BuildingKind::TownHall, CellCoord::new(0, 0))
        .expect("town hall should place");

    let has_inventory = simulation.with_surface_world(surface, |world| {
        world.get::<WarehouseInventory>(entity).is_some()
    });

    assert!(!has_inventory);
}

#[test]
fn test_warehouse_inventory_defaults_to_requested_capacity() {
    let inventory = WarehouseInventory::empty();

    assert_eq!(inventory.contents().total(), 0);
    assert_eq!(inventory.used_size(), 0);
    assert_eq!(inventory.free_size(), DEFAULT_WAREHOUSE_INVENTORY_MAX_SIZE);
    assert_eq!(inventory.max_size(), DEFAULT_WAREHOUSE_INVENTORY_MAX_SIZE);
}

#[test]
fn test_completed_warehouse_blueprint_becomes_finished_building_with_inventory() {
    let mut world = World::new();
    let blueprint = spawn_blueprint_with_progress(
        &mut world,
        BuildingKind::Warehouse,
        BuildingKind::Warehouse.definition().construction_cost(),
    );

    world
        .run_system_once(system_complete_building_construction)
        .expect("completion system should run");

    let building = world
        .get::<Building>(blueprint)
        .expect("completed blueprint should become a building");
    assert_eq!(building.kind, BuildingKind::Warehouse);
    assert!(world.get::<BuildingBlueprint>(blueprint).is_none());
    assert!(world.get::<ConstructionProgress>(blueprint).is_none());
    assert!(world.get::<WarehouseInventory>(blueprint).is_some());
}

#[test]
fn test_completed_town_hall_does_not_get_warehouse_inventory() {
    let mut world = World::new();
    let blueprint = spawn_blueprint_with_progress(
        &mut world,
        BuildingKind::TownHall,
        BuildingKind::TownHall.definition().construction_cost(),
    );

    world
        .run_system_once(system_complete_building_construction)
        .expect("completion system should run");

    assert!(world.get::<Building>(blueprint).is_some());
    assert!(world.get::<WarehouseInventory>(blueprint).is_none());
}

#[test]
fn test_completed_house_gets_capacity_and_blueprint_does_not() {
    let mut world = World::new();
    let blueprint = spawn_blueprint_with_progress(
        &mut world,
        BuildingKind::MediumHouse,
        BuildingKind::MediumHouse.definition().construction_cost(),
    );
    assert!(world.get::<House>(blueprint).is_none());

    world
        .run_system_once(system_complete_building_construction)
        .expect("completion system should run");

    assert_eq!(
        world.get::<House>(blueprint).copied(),
        Some(House::new(4, 0))
    );
}

#[test]
fn test_building_blueprint_rejects_finished_building_overlap() {
    let mut world = World::new();
    world.insert_resource(Grid::new(8, 8));
    world.spawn(Building::new(
        BuildingKind::Warehouse,
        BuildingFootprint::new(CellCoord::new(2, 2), 2, 2),
    ));

    let result = place_building_blueprint(&mut world, BuildingKind::TownHall, CellCoord::new(3, 3));

    assert_eq!(result, Err(BuildingPlacementError::OverlapsBuilding));
}

#[test]
fn test_building_blueprints_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(4, 4));

    simulation
        .place_building_blueprint(
            second_surface,
            BuildingKind::Warehouse,
            CellCoord::new(0, 0),
        )
        .expect("warehouse should place");

    assert_eq!(building_count(&simulation, default_surface), 0);
    assert_eq!(building_count(&simulation, second_surface), 1);
}

fn first_npc_coord(world: &bevy_ecs::world::World) -> Option<CellCoord> {
    let mut query = world.try_query::<(&NpcPosition, &Npc)>()?;
    query.iter(world).next().map(|(position, _)| position.coord)
}

fn first_buildable_resource_node_coord(world: &bevy_ecs::world::World) -> Option<CellCoord> {
    let mut query = world.try_query::<(
        &game_engine::components::TilePosition,
        &ResourceNode,
        &Terrain,
    )>()?;
    query
        .iter(world)
        .find(|(_, _, terrain)| terrain.kind != TerrainKind::Water)
        .map(|(position, _, _)| position.coord)
}

fn origin_for_footprint_containing(
    size: GridSize,
    coord: CellCoord,
    width: i32,
    height: i32,
) -> CellCoord {
    let max_x = size.width_i32().expect("test grid width should fit") - width;
    let max_y = size.height_i32().expect("test grid height should fit") - height;

    CellCoord::new(coord.x().clamp(0, max_x), coord.y().clamp(0, max_y))
}

fn building_count(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> usize {
    simulation.with_surface_world(surface, |world| {
        world
            .try_query::<&BuildingBlueprint>()
            .map(|mut query| query.iter(world).count())
            .unwrap_or_default()
    })
}

fn spawn_blueprint_with_progress(
    world: &mut World,
    kind: BuildingKind,
    deposited: ResourceAmounts,
) -> Entity {
    let definition = kind.definition();
    world
        .spawn((
            BuildingBlueprint {
                kind,
                footprint: BuildingFootprint::new(
                    CellCoord::new(0, 0),
                    definition.width(),
                    definition.height(),
                ),
            },
            ConstructionProgress::new(deposited),
        ))
        .id()
}
