use crate::buildings::{Building, BuildingBlueprint, BuildingKind};
use crate::components::{ResourceNode, Terrain, TerrainKind, Tile, TilePosition};
use crate::grid::CellCoord;
use crate::tile::TileIndex;
use bevy_ecs::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CollisionFlags {
    build_blocked: bool,
    npc_walk_blocked: bool,
}

impl CollisionFlags {
    pub const fn new(build_blocked: bool, npc_walk_blocked: bool) -> Self {
        Self {
            build_blocked,
            npc_walk_blocked,
        }
    }

    pub const fn is_build_blocked(self) -> bool {
        self.build_blocked
    }

    pub const fn is_npc_walk_blocked(self) -> bool {
        self.npc_walk_blocked
    }

    fn block_building(&mut self) {
        self.build_blocked = true;
    }

    fn block_npc_walk(&mut self) {
        self.npc_walk_blocked = true;
    }
}

pub fn collision_flags_at(world: &World, coord: CellCoord) -> Option<CollisionFlags> {
    let terrain = terrain_at(world, coord)?;
    let mut flags = terrain_collision_flags(terrain);

    if resource_node_at(world, coord) {
        flags.block_building();
        flags.block_npc_walk();
    }

    if let Some(mut query) = world.try_query::<&BuildingBlueprint>() {
        for blueprint in query.iter(world) {
            if blueprint.footprint.contains(coord) {
                flags.block_building();
                if building_blocks_npc_walk(blueprint.kind) {
                    flags.block_npc_walk();
                }
            }
        }
    }

    if let Some(mut query) = world.try_query::<&Building>() {
        for building in query.iter(world) {
            if building.footprint.contains(coord) {
                flags.block_building();
                if building_blocks_npc_walk(building.kind) {
                    flags.block_npc_walk();
                }
            }
        }
    }

    Some(flags)
}

pub const fn terrain_allows_building(kind: BuildingKind, terrain: TerrainKind) -> bool {
    match kind {
        BuildingKind::Depot
        | BuildingKind::Warehouse
        | BuildingKind::TownHall
        | BuildingKind::Sawmill
        | BuildingKind::Stoneworks
        | BuildingKind::Kitchen
        | BuildingKind::Farm
        | BuildingKind::SmallHouse
        | BuildingKind::MediumHouse
        | BuildingKind::LargeHouse => {
            matches!(
                terrain,
                TerrainKind::Grass | TerrainKind::Dirt | TerrainKind::Sand
            )
        }
        BuildingKind::Field => matches!(terrain, TerrainKind::Grass | TerrainKind::Dirt),
        BuildingKind::ForesterLodge | BuildingKind::TreePlot => {
            matches!(terrain, TerrainKind::Grass)
        }
    }
}

pub(crate) fn terrain_at(world: &World, coord: CellCoord) -> Option<TerrainKind> {
    let index = world.get_resource::<TileIndex>()?;
    let entity = index.get(coord)?;
    world.get::<Tile>(entity)?;
    Some(world.get::<Terrain>(entity)?.kind)
}

pub(crate) fn resource_node_at(world: &World, coord: CellCoord) -> bool {
    world
        .try_query::<(&TilePosition, &ResourceNode)>()
        .map(|mut query| {
            query
                .iter(world)
                .any(|(position, _)| position.coord == coord)
        })
        .unwrap_or(false)
}

const fn terrain_collision_flags(terrain: TerrainKind) -> CollisionFlags {
    match terrain {
        TerrainKind::Water => CollisionFlags::new(true, true),
        TerrainKind::Grass | TerrainKind::Dirt | TerrainKind::Sand => {
            CollisionFlags::new(false, false)
        }
    }
}

pub(crate) const fn building_blocks_npc_walk(kind: BuildingKind) -> bool {
    !matches!(kind, BuildingKind::Field | BuildingKind::TreePlot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingBlueprintBundle, BuildingFootprint};
    use crate::grid::{Grid, GridSize};
    use crate::resources::ResourceKind;
    use crate::tile::TileBundle;

    #[test]
    fn empty_land_tiles_do_not_block_collision_layers() {
        for terrain in [TerrainKind::Grass, TerrainKind::Dirt, TerrainKind::Sand] {
            let world = world_with_default_terrain(terrain);

            let flags = collision_flags_at(&world, CellCoord::new(1, 1))
                .expect("tile should have collision flags");

            assert!(!flags.is_build_blocked());
            assert!(!flags.is_npc_walk_blocked());
        }
    }

    #[test]
    fn water_blocks_building_and_npc_walking() {
        let world = world_with_default_terrain(TerrainKind::Water);

        let flags = collision_flags_at(&world, CellCoord::new(1, 1))
            .expect("tile should have collision flags");

        assert!(flags.is_build_blocked());
        assert!(flags.is_npc_walk_blocked());
    }

    #[test]
    fn resource_node_blocks_until_removed() {
        let mut world = world_with_default_terrain(TerrainKind::Grass);
        let coord = CellCoord::new(1, 1);
        let tile = indexed_tile_entity(&world, coord);
        world.entity_mut(tile).insert(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 10,
        });

        let blocked = collision_flags_at(&world, coord).expect("tile should have collision flags");
        assert!(blocked.is_build_blocked());
        assert!(blocked.is_npc_walk_blocked());

        world.entity_mut(tile).remove::<ResourceNode>();

        let unblocked =
            collision_flags_at(&world, coord).expect("tile should have collision flags");
        assert!(!unblocked.is_build_blocked());
        assert!(!unblocked.is_npc_walk_blocked());
    }

    #[test]
    fn major_buildings_and_blueprints_block_npc_walking() {
        let coord = CellCoord::new(2, 2);
        for kind in [
            BuildingKind::Warehouse,
            BuildingKind::TownHall,
            BuildingKind::Sawmill,
            BuildingKind::Stoneworks,
            BuildingKind::Kitchen,
            BuildingKind::Farm,
            BuildingKind::SmallHouse,
            BuildingKind::MediumHouse,
            BuildingKind::LargeHouse,
        ] {
            let mut constructed_world = world_with_default_terrain(TerrainKind::Grass);
            let footprint = BuildingFootprint::new(coord, 3, 3);
            constructed_world.spawn(Building::new(kind, footprint));

            let constructed_flags = collision_flags_at(&constructed_world, coord)
                .expect("tile should have collision flags");
            assert!(constructed_flags.is_build_blocked());
            assert!(constructed_flags.is_npc_walk_blocked());

            let mut blueprint_world = world_with_default_terrain(TerrainKind::Grass);
            blueprint_world.spawn(BuildingBlueprintBundle::new(kind, footprint));

            let blueprint_flags = collision_flags_at(&blueprint_world, coord)
                .expect("tile should have collision flags");
            assert!(blueprint_flags.is_build_blocked());
            assert!(blueprint_flags.is_npc_walk_blocked());
        }
    }

    #[test]
    fn fields_and_field_blueprints_block_building_but_not_npc_walking() {
        let coord = CellCoord::new(2, 2);
        let footprint = BuildingFootprint::new(coord, 1, 1);

        let mut constructed_world = world_with_default_terrain(TerrainKind::Grass);
        constructed_world.spawn(Building::new(BuildingKind::Field, footprint));

        let constructed_flags = collision_flags_at(&constructed_world, coord)
            .expect("tile should have collision flags");
        assert!(constructed_flags.is_build_blocked());
        assert!(!constructed_flags.is_npc_walk_blocked());

        let mut blueprint_world = world_with_default_terrain(TerrainKind::Grass);
        blueprint_world.spawn(BuildingBlueprintBundle::new(BuildingKind::Field, footprint));

        let blueprint_flags =
            collision_flags_at(&blueprint_world, coord).expect("tile should have collision flags");
        assert!(blueprint_flags.is_build_blocked());
        assert!(!blueprint_flags.is_npc_walk_blocked());
    }

    fn world_with_default_terrain(terrain: TerrainKind) -> World {
        let size = GridSize::new(8, 8);
        let mut world = World::new();
        world.insert_resource(Grid::new(size.width(), size.height()));
        let mut index = TileIndex::new(size);
        for coord in size.iter_coords() {
            let entity = world
                .spawn(TileBundle::new_with_terrain(coord, terrain))
                .id();
            assert!(index.set(coord, entity));
        }
        world.insert_resource(index);
        world
    }

    fn indexed_tile_entity(world: &World, coord: CellCoord) -> Entity {
        world
            .resource::<TileIndex>()
            .get(coord)
            .expect("test tile should exist in index")
    }
}
