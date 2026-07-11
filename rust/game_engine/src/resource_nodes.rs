pub use crate::components::ResourceNode;

use crate::grid::{CellCoord, Grid, GridSize};
use crate::resources::ResourceKind;
use crate::tile::TileIndex;
use bevy_ecs::prelude::*;

const COVERAGE_PER_THOUSAND: usize = 15;
const MIN_RESOURCE_NODE_QUANTITY: u32 = 50;
const RESOURCE_NODE_QUANTITY_RANGE: u64 = 101;

pub fn spawn_initial_resource_nodes(
    mut commands: Commands,
    grid: Res<Grid>,
    index: Res<TileIndex>,
) {
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
        let Some(entity) = index.get(coord) else {
            continue;
        };

        commands.entity(entity).insert(ResourceNode {
            kind: resource_kind_for_hash(hash),
            quantity: resource_quantity_for_hash(hash),
        });
    }
}

fn resource_kind_for_hash(hash: u64) -> ResourceKind {
    ResourceKind::NATURAL[((hash >> 32) % ResourceKind::NATURAL.len() as u64) as usize]
}

fn resource_quantity_for_hash(hash: u64) -> u32 {
    MIN_RESOURCE_NODE_QUANTITY + (hash % RESOURCE_NODE_QUANTITY_RANGE) as u32
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_resource_bucket_order_is_stable() {
        let bucket_hash = |bucket: u64| bucket << 32;

        assert_eq!(resource_kind_for_hash(bucket_hash(0)), ResourceKind::Wood);
        assert_eq!(resource_kind_for_hash(bucket_hash(1)), ResourceKind::Stone);
        assert_eq!(
            resource_kind_for_hash(bucket_hash(2)),
            ResourceKind::WildBerries
        );
        assert_eq!(resource_kind_for_hash(bucket_hash(3)), ResourceKind::Gold);
    }
}
