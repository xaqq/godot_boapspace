use game_engine::grid::{CellCoord, CellType, Grid, GridSize, WorldPosition};

#[test]
fn test_large_grid_creation() {
    let g = Grid::new(256, 256);
    assert_eq!(g.size(), GridSize::new(256, 256));
}

#[test]
fn test_set_all_building_then_read_back() {
    let mut g = Grid::new(10, 10);
    for y in 0..10i32 {
        for x in 0..10i32 {
            assert!(g.set(CellCoord::new(x, y), CellType::Building));
        }
    }
    for y in 0..10i32 {
        for x in 0..10i32 {
            assert_eq!(g.get(CellCoord::new(x, y)), Some(CellType::Building));
        }
    }
}

#[test]
fn test_world_cell_roundtrip() {
    let world = Grid::cell_to_world(CellCoord::new(5, 3));
    let cell = Grid::world_to_cell(world, GridSize::new(256, 256));
    assert_eq!(cell, Some(CellCoord::new(5, 3)));
}

#[test]
fn test_cell_to_world_centered() {
    let world = Grid::cell_to_world(CellCoord::new(0, 0));
    // First cell center is at half tile size
    assert!((world.x() - 32.0).abs() < 0.001);
    assert!((world.y() - 32.0).abs() < 0.001);

    let world = Grid::cell_to_world(CellCoord::new(1, 0));
    assert!((world.x() - 96.0).abs() < 0.001);
}

#[test]
fn test_world_to_cell_rejects_non_finite_positions() {
    let cell = Grid::world_to_cell(
        WorldPosition::new(f32::INFINITY, 0.0),
        GridSize::new(256, 256),
    );
    assert_eq!(cell, None);
}
