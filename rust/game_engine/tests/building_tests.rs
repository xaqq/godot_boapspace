use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingBlueprintKind, BuildingFootprint, BuildingPlacementError,
    ConstructionProgress, WarehouseInventory,
};
use game_engine::grid::{CellCoord, GridSize};
use game_engine::npcs::{Npc, NpcPosition};
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::ResourceKind;
use game_engine::simulation::GameSimulation;

#[test]
fn test_building_definitions_include_dimensions_and_costs() {
    let warehouse = BuildingBlueprintKind::Warehouse.definition();
    assert_eq!(warehouse.kind(), BuildingBlueprintKind::Warehouse);
    assert_eq!(warehouse.width(), 2);
    assert_eq!(warehouse.height(), 2);
    assert_eq!(warehouse.construction_cost().get(ResourceKind::Wood), 40);
    assert_eq!(warehouse.construction_cost().get(ResourceKind::Stone), 20);
    assert_eq!(warehouse.construction_cost().get(ResourceKind::Food), 0);
    assert_eq!(warehouse.construction_cost().get(ResourceKind::Gold), 0);

    let town_hall = BuildingBlueprintKind::TownHall.definition();
    assert_eq!(town_hall.kind(), BuildingBlueprintKind::TownHall);
    assert_eq!(town_hall.width(), 3);
    assert_eq!(town_hall.height(), 3);
    assert_eq!(town_hall.construction_cost().get(ResourceKind::Wood), 80);
    assert_eq!(town_hall.construction_cost().get(ResourceKind::Stone), 60);
    assert_eq!(town_hall.construction_cost().get(ResourceKind::Food), 0);
    assert_eq!(town_hall.construction_cost().get(ResourceKind::Gold), 20);
}

#[test]
fn test_place_building_blueprint_inside_bounds() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));

    let entity = simulation
        .place_building_blueprint(
            surface,
            BuildingBlueprintKind::Warehouse,
            CellCoord::new(1, 1),
        )
        .expect("warehouse should place inside bounds");

    let info = simulation
        .with_surface_world(surface, |world| {
            let building = world.get::<Building>(entity)?;
            let footprint = world.get::<BuildingFootprint>(entity)?;
            world.get::<BuildingBlueprint>(entity)?;
            Some((building.kind, *footprint))
        })
        .expect("placed building should be queryable");

    assert_eq!(info.0, BuildingBlueprintKind::Warehouse);
    assert_eq!(info.1.origin(), CellCoord::new(1, 1));
    assert_eq!(info.1.width(), 2);
    assert_eq!(info.1.height(), 2);
}

#[test]
fn test_place_building_blueprint_rejects_out_of_bounds() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));

    let result = simulation.place_building_blueprint(
        surface,
        BuildingBlueprintKind::TownHall,
        CellCoord::new(2, 2),
    );

    assert_eq!(result, Err(BuildingPlacementError::OutOfBounds));
}

#[test]
fn test_place_building_blueprint_rejects_building_overlap() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(8, 8));

    simulation
        .place_building_blueprint(
            surface,
            BuildingBlueprintKind::Warehouse,
            CellCoord::new(2, 2),
        )
        .expect("first building should place");
    let result = simulation.place_building_blueprint(
        surface,
        BuildingBlueprintKind::TownHall,
        CellCoord::new(3, 3),
    );

    assert_eq!(result, Err(BuildingPlacementError::OverlapsBuilding));
}

#[test]
fn test_blueprint_can_overlap_npc() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();
    let npc_coord = simulation
        .with_surface_world(surface, first_npc_coord)
        .expect("default surface should have an NPC");

    let result =
        simulation.place_building_blueprint(surface, BuildingBlueprintKind::Warehouse, npc_coord);

    assert!(result.is_ok());
}

#[test]
fn test_blueprint_can_overlap_resource_node() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();
    let size = simulation.grid_size(surface);
    let resource_coord = simulation
        .with_surface_world(surface, first_resource_node_coord)
        .expect("default surface should have resource nodes");
    let origin = origin_for_footprint_containing(size, resource_coord, 2, 2);
    let footprint = BuildingFootprint::new(origin, 2, 2);

    assert!(footprint.contains(resource_coord));
    let result =
        simulation.place_building_blueprint(surface, BuildingBlueprintKind::Warehouse, origin);

    assert!(result.is_ok());
}

#[test]
fn test_construction_progress_starts_empty() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let entity = simulation
        .place_building_blueprint(
            surface,
            BuildingBlueprintKind::TownHall,
            CellCoord::new(0, 0),
        )
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
        .place_building_blueprint(
            surface,
            BuildingBlueprintKind::Warehouse,
            CellCoord::new(0, 0),
        )
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
        .place_building_blueprint(
            surface,
            BuildingBlueprintKind::TownHall,
            CellCoord::new(0, 0),
        )
        .expect("town hall should place");

    let has_inventory = simulation.with_surface_world(surface, |world| {
        world.get::<WarehouseInventory>(entity).is_some()
    });

    assert!(!has_inventory);
}

#[test]
fn test_building_blueprints_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(4, 4));

    simulation
        .place_building_blueprint(
            second_surface,
            BuildingBlueprintKind::Warehouse,
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

fn first_resource_node_coord(world: &bevy_ecs::world::World) -> Option<CellCoord> {
    let mut query = world.try_query::<(&game_engine::components::TilePosition, &ResourceNode)>()?;
    query.iter(world).next().map(|(position, _)| position.coord)
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
            .try_query::<(&Building, &BuildingFootprint)>()
            .map(|mut query| query.iter(world).count())
            .unwrap_or_default()
    })
}
