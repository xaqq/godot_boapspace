use game_engine::components::{Terrain, TerrainKind, Tile, TileDisplay, TilePosition};
use game_engine::grid::{CellCoord, GridSize};
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::{GameResources, ResourceKind};
use game_engine::simulation::{GameSimulation, DEFAULT_GRID_SIZE};
use std::collections::HashSet;

#[test]
fn test_new_creates_default_surface() {
    let simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();

    assert_eq!(simulation.surface_count(), 1);
    assert_eq!(simulation.grid_size(surface), Some(DEFAULT_GRID_SIZE));
}

#[test]
fn test_create_surface_returns_distinct_id() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(10, 12));

    assert_ne!(default_surface, second_surface);
    assert_eq!(simulation.surface_count(), 2);
    assert_eq!(
        simulation.grid_size(second_surface),
        Some(GridSize::new(10, 12))
    );
}

#[test]
fn test_surface_id_at_returns_valid_surface_ids() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(10, 12));

    assert_eq!(simulation.surface_id_at(0), Some(default_surface));
    assert_eq!(simulation.surface_id_at(1), Some(second_surface));
}

#[test]
fn test_surface_id_at_rejects_invalid_indexes() {
    let mut simulation = GameSimulation::new();
    simulation.create_surface(GridSize::new(10, 12));

    assert_eq!(simulation.surface_id_at(2), None);
}

#[test]
fn test_resources_are_available_per_surface() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(8, 8));

    let default_amounts = simulation.with_surface_world(default_surface, resource_amounts);
    let second_amounts = simulation.with_surface_world(second_surface, resource_amounts);

    assert_eq!(default_amounts, Some([0, 0, 0, 0]));
    assert_eq!(second_amounts, Some([0, 0, 0, 0]));
}

#[test]
fn test_surface_world_read_closure_can_read_resources() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(8, 8));

    let default_food = simulation.with_surface_world(default_surface, |world| {
        world.resource::<GameResources>().get(ResourceKind::Food)
    });
    let second_gold = simulation.with_surface_world(second_surface, |world| {
        world.resource::<GameResources>().get(ResourceKind::Gold)
    });

    assert_eq!(default_food, Some(0));
    assert_eq!(second_gold, Some(0));
}

#[test]
fn test_tile_display_reads_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(4, 5));

    let default_cell = simulation.tile_display_at(default_surface, CellCoord::new(100, 100));
    let second_cell = simulation.tile_display_at(second_surface, CellCoord::new(100, 100));
    let second_valid_cell = simulation.tile_display_at(second_surface, CellCoord::new(3, 4));

    assert_eq!(default_cell, Some(TileDisplay::Empty));
    assert_eq!(second_cell, None);
    assert_eq!(second_valid_cell, Some(TileDisplay::Empty));
}

#[test]
fn test_tick_runs_across_multiple_surfaces() {
    let mut simulation = GameSimulation::new();
    simulation.create_surface(GridSize::new(6, 6));

    simulation.tick(1.0 / 60.0);

    assert_eq!(simulation.surface_count(), 2);
}

#[test]
fn test_surface_spawns_one_tile_entity_per_cell() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 5));

    let tiles = tiles(&simulation, surface);

    assert_eq!(tiles.len(), 20);
}

#[test]
fn test_tile_entities_are_unique_within_bounds_and_grass() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(7, 9));
    let size = simulation.grid_size(surface).expect("surface should exist");
    let tiles = tiles(&simulation, surface);
    let unique_tiles = tiles
        .iter()
        .map(|(coord, _)| *coord)
        .collect::<HashSet<_>>();

    assert_eq!(tiles.len(), unique_tiles.len());
    assert_eq!(
        tiles.len(),
        size.cell_count().expect("grid size should fit")
    );
    for (coord, terrain) in tiles {
        assert!(size.contains(coord), "{coord:?} should be within {size:?}");
        assert_eq!(terrain, TerrainKind::Grass);
    }
}

#[test]
fn test_default_and_created_surfaces_have_resource_nodes() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(10, 12));

    assert!(!resource_nodes(&mut simulation, default_surface).is_empty());
    assert!(!resource_nodes(&mut simulation, second_surface).is_empty());
}

#[test]
fn test_resource_nodes_are_within_bounds() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(17, 19));
    let size = simulation.grid_size(surface).expect("surface should exist");

    for (coord, _) in resource_nodes(&mut simulation, surface) {
        assert!(size.contains(coord), "{coord:?} should be within {size:?}");
    }
}

#[test]
fn test_resource_nodes_do_not_share_tiles() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();
    let nodes = resource_nodes(&mut simulation, surface);
    let unique_tiles = nodes
        .iter()
        .map(|(coord, _)| *coord)
        .collect::<HashSet<_>>();

    assert_eq!(nodes.len(), unique_tiles.len());
}

#[test]
fn test_resource_node_generation_is_deterministic_for_same_size() {
    let mut first = GameSimulation::new();
    let mut second = GameSimulation::new();
    let first_default = first.default_surface_id();
    let second_default = second.default_surface_id();

    assert_eq!(
        sorted_resource_nodes(&mut first, first_default),
        sorted_resource_nodes(&mut second, second_default)
    );

    let first_surface = first.create_surface(GridSize::new(31, 29));
    let second_surface = second.create_surface(GridSize::new(31, 29));

    assert_eq!(
        sorted_resource_nodes(&mut first, first_surface),
        sorted_resource_nodes(&mut second, second_surface)
    );
}

#[test]
fn test_resource_node_queries_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(7, 9));

    assert_ne!(
        sorted_resource_nodes(&mut simulation, default_surface),
        sorted_resource_nodes(&mut simulation, second_surface)
    );
}

#[test]
fn test_tick_does_not_duplicate_resource_nodes() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.default_surface_id();
    let before = sorted_resource_nodes(&mut simulation, surface);

    simulation.tick(1.0 / 60.0);
    simulation.tick(1.0 / 60.0);

    assert_eq!(sorted_resource_nodes(&mut simulation, surface), before);
}

fn sorted_resource_nodes(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> Vec<(CellCoord, ResourceKind)> {
    let mut nodes = resource_nodes(simulation, surface);
    nodes.sort_unstable_by_key(|(coord, kind)| (coord.y(), coord.x(), *kind as u8));
    nodes
}

fn resource_nodes(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> Vec<(CellCoord, ResourceKind)> {
    simulation
        .with_surface_world(surface, query_resource_nodes)
        .expect("surface should exist")
}

fn tiles(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> Vec<(CellCoord, TerrainKind)> {
    simulation
        .with_surface_world(surface, query_tiles)
        .expect("surface should exist")
}

fn resource_amounts(world: &bevy_ecs::world::World) -> [u32; ResourceKind::ALL.len()] {
    let resources = world.resource::<GameResources>();
    ResourceKind::ALL.map(|kind| resources.get(kind))
}

fn query_resource_nodes(world: &bevy_ecs::world::World) -> Vec<(CellCoord, ResourceKind)> {
    world
        .try_query::<(&TilePosition, &ResourceNode)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(position, node)| (position.coord, node.kind))
                .collect()
        })
        .unwrap_or_default()
}

fn query_tiles(world: &bevy_ecs::world::World) -> Vec<(CellCoord, TerrainKind)> {
    world
        .try_query::<(&TilePosition, &Terrain, &Tile)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(position, terrain, _)| (position.coord, terrain.kind))
                .collect()
        })
        .unwrap_or_default()
}
