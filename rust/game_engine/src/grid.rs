use bevy_ecs::prelude::Resource;

pub const TILE_SIZE: f32 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellCoord {
    x: i32,
    y: i32,
}

impl CellCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn from_usize(x: usize, y: usize) -> Option<Self> {
        Some(Self {
            x: i32::try_from(x).ok()?,
            y: i32::try_from(y).ok()?,
        })
    }

    pub const fn x(self) -> i32 {
        self.x
    }

    pub const fn y(self) -> i32 {
        self.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPosition {
    x: f32,
    y: f32,
}

impl WorldPosition {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const fn x(self) -> f32 {
        self.x
    }

    pub const fn y(self) -> f32 {
        self.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridSize {
    width: usize,
    height: usize,
}

impl GridSize {
    pub const fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }

    pub const fn width(self) -> usize {
        self.width
    }

    pub const fn height(self) -> usize {
        self.height
    }

    pub fn width_i32(self) -> Option<i32> {
        i32::try_from(self.width).ok()
    }

    pub fn height_i32(self) -> Option<i32> {
        i32::try_from(self.height).ok()
    }

    pub fn cell_count(self) -> Option<usize> {
        self.width.checked_mul(self.height)
    }

    pub fn contains(self, coord: CellCoord) -> bool {
        let Ok(x) = usize::try_from(coord.x) else {
            return false;
        };
        let Ok(y) = usize::try_from(coord.y) else {
            return false;
        };

        x < self.width && y < self.height
    }

    pub fn iter_coords(self) -> impl Iterator<Item = CellCoord> {
        (0..self.height)
            .flat_map(move |y| (0..self.width).filter_map(move |x| CellCoord::from_usize(x, y)))
    }
}

#[derive(Debug, Clone, Resource)]
pub struct Grid {
    size: GridSize,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        let size = GridSize::new(width, height);

        Self { size }
    }

    pub const fn size(&self) -> GridSize {
        self.size
    }

    pub const fn width(&self) -> usize {
        self.size.width()
    }

    pub const fn height(&self) -> usize {
        self.size.height()
    }

    pub fn cell_to_world(coord: CellCoord) -> WorldPosition {
        WorldPosition::new(
            (coord.x() as f32 + 0.5) * TILE_SIZE,
            (coord.y() as f32 + 0.5) * TILE_SIZE,
        )
    }

    pub fn world_to_cell(world: WorldPosition, size: GridSize) -> Option<CellCoord> {
        if !world.x().is_finite() || !world.y().is_finite() {
            return None;
        }

        let x = (world.x() / TILE_SIZE).floor();
        let y = (world.y() / TILE_SIZE).floor();

        if x < i32::MIN as f32 || x > i32::MAX as f32 || y < i32::MIN as f32 || y > i32::MAX as f32
        {
            return None;
        }

        let coord = CellCoord::new(x as i32, y as i32);
        size.contains(coord).then_some(coord)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_new() {
        let g = Grid::new(10, 5);
        assert_eq!(g.width(), 10);
        assert_eq!(g.height(), 5);
    }

    #[test]
    fn test_contains_bounds() {
        let g = Grid::new(10, 10);
        assert!(g.size().contains(CellCoord::new(5, 5)));
        assert!(!g.size().contains(CellCoord::new(-1, 0)));
        assert!(!g.size().contains(CellCoord::new(10, 0)));
        assert!(!g.size().contains(CellCoord::new(0, -1)));
        assert!(!g.size().contains(CellCoord::new(0, 10)));
    }

    #[test]
    fn test_iter_coords_matches_cell_count() {
        let size = GridSize::new(10, 10);
        assert_eq!(Some(size.iter_coords().count()), size.cell_count());
    }

    #[test]
    fn test_world_to_cell() {
        let result = Grid::world_to_cell(WorldPosition::new(96.0, 96.0), GridSize::new(256, 256));
        assert_eq!(result, Some(CellCoord::new(1, 1)));
    }

    #[test]
    fn test_world_to_cell_out_of_bounds() {
        let result = Grid::world_to_cell(WorldPosition::new(-1.0, 0.0), GridSize::new(256, 256));
        assert_eq!(result, None);
    }

    #[test]
    fn test_world_to_cell_rejects_non_finite_positions() {
        let result =
            Grid::world_to_cell(WorldPosition::new(f32::NAN, 0.0), GridSize::new(256, 256));
        assert_eq!(result, None);
    }

    #[test]
    fn test_cell_to_world() {
        let world = Grid::cell_to_world(CellCoord::new(0, 0));
        assert_eq!(world.x(), 32.0);
        assert_eq!(world.y(), 32.0);
    }
}
