pub use crate::components::ResourceNode;

use crate::components::{Terrain, TerrainKind};
use crate::grid::Grid;
use crate::resources::ResourceKind;
use crate::tile::{generation_hash, SurfaceGeneration, TileIndex};
use bevy_ecs::prelude::*;

const COVERAGE_PER_THOUSAND: usize = 15;
const MIN_RESOURCE_NODE_QUANTITY: u32 = 50;
const RESOURCE_NODE_QUANTITY_RANGE: u64 = 101;
const PLACEMENT_DOMAIN: u64 = 0x3c6e_f372_fe94_f82b;
const KIND_DOMAIN: u64 = 0xa54f_f53a_5f1d_36f1;
const QUANTITY_DOMAIN: u64 = 0x510e_527f_ade6_82d1;
const GRASS_RESOURCES: [ResourceKind; 2] = [ResourceKind::Wood, ResourceKind::WildBerries];
const DIRT_RESOURCES: [ResourceKind; 4] = [
    ResourceKind::Wood,
    ResourceKind::WildBerries,
    ResourceKind::Stone,
    ResourceKind::Gold,
];
const SAND_RESOURCES: [ResourceKind; 2] = [ResourceKind::Stone, ResourceKind::Gold];

pub(crate) fn spawn_initial_resource_nodes(
    mut commands: Commands,
    grid: Res<Grid>,
    index: Res<TileIndex>,
    generation: Res<SurfaceGeneration>,
    terrain_query: Query<&Terrain>,
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
        .filter(|&coord| !generation.protects(size, coord))
        .filter_map(|coord| {
            let entity = index.get(coord)?;
            let terrain = terrain_query.get(entity).ok()?.kind;
            let allowed_kinds = resource_kinds_for_terrain(terrain);
            if allowed_kinds.is_empty() {
                return None;
            }

            let kind_hash = generation_hash(generation.seed(), coord, KIND_DOMAIN);
            let kind = allowed_kinds[(kind_hash % allowed_kinds.len() as u64) as usize];
            let placement = generation_hash(generation.seed(), coord, PLACEMENT_DOMAIN);
            let quantity_hash = generation_hash(generation.seed(), coord, QUANTITY_DOMAIN);
            Some((placement, coord, entity, kind, quantity_hash))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|(hash, coord, _, _, _)| (*hash, coord.y(), coord.x()));

    for (_, _, entity, kind, quantity_hash) in candidates.into_iter().take(target_count) {
        commands.entity(entity).insert(ResourceNode {
            kind,
            quantity: resource_quantity_for_hash(quantity_hash),
        });
    }
}

pub const fn terrain_allows_resource(terrain: TerrainKind, resource: ResourceKind) -> bool {
    match resource {
        ResourceKind::Wood | ResourceKind::WildBerries => {
            matches!(terrain, TerrainKind::Grass | TerrainKind::Dirt)
        }
        ResourceKind::Stone | ResourceKind::Gold => {
            matches!(terrain, TerrainKind::Dirt | TerrainKind::Sand)
        }
        ResourceKind::Planks
        | ResourceKind::Crops
        | ResourceKind::Food
        | ResourceKind::StoneBlocks => false,
    }
}

fn resource_quantity_for_hash(hash: u64) -> u32 {
    MIN_RESOURCE_NODE_QUANTITY + (hash % RESOURCE_NODE_QUANTITY_RANGE) as u32
}

const fn resource_kinds_for_terrain(terrain: TerrainKind) -> &'static [ResourceKind] {
    match terrain {
        TerrainKind::Grass => &GRASS_RESOURCES,
        TerrainKind::Dirt => &DIRT_RESOURCES,
        TerrainKind::Sand => &SAND_RESOURCES,
        TerrainKind::Water => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_resources_only_allow_their_ecological_terrain() {
        for terrain in TerrainKind::ALL {
            for resource in ResourceKind::ALL {
                let expected = match resource {
                    ResourceKind::Wood | ResourceKind::WildBerries => {
                        matches!(terrain, TerrainKind::Grass | TerrainKind::Dirt)
                    }
                    ResourceKind::Stone | ResourceKind::Gold => {
                        matches!(terrain, TerrainKind::Dirt | TerrainKind::Sand)
                    }
                    _ => false,
                };
                assert_eq!(terrain_allows_resource(terrain, resource), expected);
            }
        }
    }
}
