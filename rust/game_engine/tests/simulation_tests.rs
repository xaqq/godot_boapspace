use game_engine::grid::{CellCoord, CellType, GridSize};
use game_engine::resources::ResourceKind;
use game_engine::simulation::{GameSimulation, DEFAULT_GRID_SIZE};

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
    let second_surface = simulation
        .create_surface(GridSize::new(10, 12))
        .expect("surface size should be valid");

    assert_ne!(default_surface, second_surface);
    assert_eq!(simulation.surface_count(), 2);
    assert_eq!(
        simulation.grid_size(second_surface),
        Some(GridSize::new(10, 12))
    );
}

#[test]
fn test_resources_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation
        .create_surface(GridSize::new(8, 8))
        .expect("surface size should be valid");

    assert_eq!(
        simulation.add_resource(default_surface, ResourceKind::Wood, 25),
        Some(true)
    );
    assert_eq!(
        simulation.add_resource(second_surface, ResourceKind::Stone, 40),
        Some(true)
    );

    assert_eq!(
        simulation.resource_amount(default_surface, ResourceKind::Wood),
        Some(25)
    );
    assert_eq!(
        simulation.resource_amount(default_surface, ResourceKind::Stone),
        Some(0)
    );
    assert_eq!(
        simulation.resource_amount(second_surface, ResourceKind::Wood),
        Some(0)
    );
    assert_eq!(
        simulation.resource_amount(second_surface, ResourceKind::Stone),
        Some(40)
    );
}

#[test]
fn test_grid_reads_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation
        .create_surface(GridSize::new(4, 5))
        .expect("surface size should be valid");

    assert_eq!(
        simulation.cell_type(default_surface, CellCoord::new(100, 100)),
        Some(CellType::Empty)
    );
    assert_eq!(
        simulation.cell_type(second_surface, CellCoord::new(100, 100)),
        None
    );
}

#[test]
fn test_tick_runs_across_multiple_surfaces() {
    let mut simulation = GameSimulation::new();
    simulation
        .create_surface(GridSize::new(6, 6))
        .expect("surface size should be valid");

    simulation.tick(1.0 / 60.0);

    assert_eq!(simulation.surface_count(), 2);
}
