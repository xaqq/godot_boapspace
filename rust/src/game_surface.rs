use godot::prelude::*;

pub const TILE_SIZE: f32 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellType {
    #[default]
    Empty,
    Building,
}

#[derive(Debug, Clone)]
pub struct GameSurface {
    pub width: usize,
    pub height: usize,
    cells: Vec<CellType>,
}

impl GameSurface {
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

    pub fn cell_to_world(x: i32, y: i32) -> Vector2 {
        Vector2::new(
            (x as f32 + 0.5) * TILE_SIZE,
            (y as f32 + 0.5) * TILE_SIZE,
        )
    }

    pub fn world_to_cell(world: Vector2, width: i32, height: i32) -> Option<(i32, i32)> {
        let x = (world.x / TILE_SIZE).floor() as i32;
        let y = (world.y / TILE_SIZE).floor() as i32;
        if x < 0 || y < 0 || x >= width || y >= height {
            None
        } else {
            Some((x, y))
        }
    }

    pub fn world_size(&self) -> Vector2 {
        Vector2::new(
            self.width as f32 * TILE_SIZE,
            self.height as f32 * TILE_SIZE,
        )
    }
}
