pub const TILE_SIZE: f32 = 64.0;

use bevy_ecs::prelude::Resource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellType {
    #[default]
    Empty,
    Building,
}

impl CellType {
    pub fn type_name(&self) -> &'static str {
        match self {
            CellType::Empty => "Empty",
            CellType::Building => "Building",
        }
    }
}

#[derive(Debug, Clone, Resource)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    cells: Vec<CellType>,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![CellType::Empty; width * height],
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            None
        } else {
            Some(y as usize * self.width + x as usize)
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<CellType> {
        self.index(x, y).map(|i| self.cells[i])
    }

    pub fn set(&mut self, x: i32, y: i32, cell: CellType) -> bool {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = cell;
            true
        } else {
            false
        }
    }

    pub fn cell_to_world(x: i32, y: i32) -> (f32, f32) {
        (
            (x as f32 + 0.5) * TILE_SIZE,
            (y as f32 + 0.5) * TILE_SIZE,
        )
    }

    pub fn world_to_cell(world_x: f32, world_y: f32, width: i32, height: i32) -> Option<(i32, i32)> {
        let x = (world_x / TILE_SIZE).floor() as i32;
        let y = (world_y / TILE_SIZE).floor() as i32;
        if x < 0 || y < 0 || x >= width || y >= height {
            None
        } else {
            Some((x, y))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_new() {
        let g = Grid::new(10, 5);
        assert_eq!(g.width, 10);
        assert_eq!(g.height, 5);
        assert_eq!(g.cells.len(), 50);
    }

    #[test]
    fn test_get_in_bounds() {
        let g = Grid::new(10, 10);
        assert_eq!(g.get(5, 5), Some(CellType::Empty));
    }

    #[test]
    fn test_get_out_of_bounds() {
        let g = Grid::new(10, 10);
        assert_eq!(g.get(-1, 0), None);
        assert_eq!(g.get(10, 0), None);
        assert_eq!(g.get(0, -1), None);
        assert_eq!(g.get(0, 10), None);
    }

    #[test]
    fn test_set_and_get() {
        let mut g = Grid::new(10, 10);
        assert!(g.set(3, 4, CellType::Building));
        assert_eq!(g.get(3, 4), Some(CellType::Building));
    }

    #[test]
    fn test_set_out_of_bounds_fails() {
        let mut g = Grid::new(10, 10);
        assert!(!g.set(10, 0, CellType::Building));
        assert!(!g.set(-1, 0, CellType::Building));
    }

    #[test]
    fn test_world_to_cell() {
        let result = Grid::world_to_cell(96.0, 96.0, 256, 256);
        assert_eq!(result, Some((1, 1)));
    }

    #[test]
    fn test_world_to_cell_out_of_bounds() {
        let result = Grid::world_to_cell(-1.0, 0.0, 256, 256);
        assert_eq!(result, None);
    }

    #[test]
    fn test_cell_to_world() {
        let (wx, wy) = Grid::cell_to_world(0, 0);
        assert_eq!(wx, 32.0);
        assert_eq!(wy, 32.0);
    }
}
