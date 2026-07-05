use crate::grid::CellCoord;
use crate::resources::ResourceKind;
use bevy_ecs::prelude::Component;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct TilePosition {
    pub coord: CellCoord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Tile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Terrain {
    pub kind: TerrainKind,
}

impl Terrain {
    pub const fn new(kind: TerrainKind) -> Self {
        Self { kind }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainKind {
    #[default]
    Grass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TileDisplay {
    #[default]
    Empty,
}

impl TileDisplay {
    pub const fn type_name(self) -> &'static str {
        match self {
            TileDisplay::Empty => "Empty",
        }
    }
}

impl From<TerrainKind> for TileDisplay {
    fn from(_kind: TerrainKind) -> Self {
        Self::Empty
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileDisplayEntry {
    pub coord: CellCoord,
    pub display: TileDisplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct ResourceNode {
    pub kind: ResourceKind,
}
