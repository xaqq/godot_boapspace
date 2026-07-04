use crate::grid::{CellCoord, Grid, GridSize};
use crate::resources::ResourceKind;
use bevy_ecs::prelude::*;

const COVERAGE_PER_THOUSAND: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct ResourceNode {
    pub kind: ResourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct TilePosition {
    pub coord: CellCoord,
}

pub fn spawn_initial_resource_nodes(mut commands: Commands, grid: Res<Grid>) {
    let size = grid.size();
    let Some(cell_count) = size.cell_count() else {
        return;
    };
    if cell_count == 0 {
        return;
    }

    let target_count = ((cell_count / 1000) * COVERAGE_PER_THOUSAND
        + ((cell_count % 1000) * COVERAGE_PER_THOUSAND) / 1000)
        .max(1);
    let mut candidates = size
        .iter_coords()
        .map(|coord| (placement_hash(size, coord), coord))
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|(hash, coord)| (*hash, coord.y(), coord.x()));

    for (hash, coord) in candidates.into_iter().take(target_count) {
        commands.spawn((
            TilePosition { coord },
            ResourceNode {
                kind: resource_kind_for_hash(hash),
            },
        ));
    }
}

fn resource_kind_for_hash(hash: u64) -> ResourceKind {
    match (hash >> 32) % ResourceKind::ALL.len() as u64 {
        0 => ResourceKind::Wood,
        1 => ResourceKind::Stone,
        2 => ResourceKind::Food,
        _ => ResourceKind::Gold,
    }
}

fn placement_hash(size: GridSize, coord: CellCoord) -> u64 {
    let mut value = 0x9e37_79b9_7f4a_7c15_u64;
    value ^= (size.width() as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = value.rotate_left(27);
    value ^= (size.height() as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    value = value.rotate_left(31);
    value ^= (coord.x() as i64 as u64).wrapping_mul(0xd6e8_feb8_6659_fd93);
    value = value.rotate_left(23);
    value ^= (coord.y() as i64 as u64).wrapping_mul(0xa5a3_58f1_d6a1_0f41);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
