use game_engine::grid::{CellType, Grid};

#[test]
fn test_large_grid_creation() {
    let g = Grid::new(256, 256);
    assert_eq!(g.width, 256);
    assert_eq!(g.height, 256);
}

#[test]
fn test_set_all_building_then_read_back() {
    let mut g = Grid::new(10, 10);
    for y in 0..10i32 {
        for x in 0..10i32 {
            g.set(x, y, CellType::Building);
        }
    }
    for y in 0..10i32 {
        for x in 0..10i32 {
            assert_eq!(g.get(x, y), Some(CellType::Building));
        }
    }
}

#[test]
fn test_world_cell_roundtrip() {
    let (wx, wy) = Grid::cell_to_world(5, 3);
    let cell = Grid::world_to_cell(wx, wy, 256, 256);
    assert_eq!(cell, Some((5, 3)));
}

#[test]
fn test_cell_to_world_centered() {
    let (wx, wy) = Grid::cell_to_world(0, 0);
    // First cell center is at half tile size
    assert!((wx - 32.0).abs() < 0.001);
    assert!((wy - 32.0).abs() < 0.001);

    let (wx, _wy) = Grid::cell_to_world(1, 0);
    assert!((wx - 96.0).abs() < 0.001);
}
