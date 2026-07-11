use bevy_ecs::prelude::*;
use game_engine::buildings::BuildingFootprint;
use game_engine::components::{MovementTarget, Npc, NpcPosition, ResourceNode, TerrainKind};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::navigation::{drive_npc_routes, NavigationSnapshot, NpcRoute};
use game_engine::resources::ResourceKind;
use game_engine::tile::{TileBundle, TileIndex};

#[test]
fn cardinal_bfs_routes_around_collision_deterministically() {
    let mut world = navigation_world(5, 3);
    set_resource_node(&mut world, CellCoord::new(2, 1), true);
    let navigation = NavigationSnapshot::from_world(&world).expect("snapshot should exist");

    let path = navigation
        .shortest_path(CellCoord::new(1, 1), CellCoord::new(3, 1))
        .expect("goal should remain reachable");

    assert_eq!(
        path,
        vec![
            CellCoord::new(1, 1),
            CellCoord::new(1, 0),
            CellCoord::new(2, 0),
            CellCoord::new(3, 0),
            CellCoord::new(3, 1),
        ]
    );
    assert!(path
        .windows(2)
        .all(|pair| cardinal_distance(pair[0], pair[1]) == 1));
}

#[test]
fn target_selection_excludes_unreachable_and_uses_row_major_ties() {
    let mut world = navigation_world(5, 5);
    for coord in [
        CellCoord::new(3, 2),
        CellCoord::new(4, 1),
        CellCoord::new(4, 3),
    ] {
        set_resource_node(&mut world, coord, true);
    }
    let navigation = NavigationSnapshot::from_world(&world).expect("snapshot should exist");

    let selected = navigation
        .shortest_path_to_any(
            CellCoord::new(2, 2),
            [
                CellCoord::new(4, 2), // unreachable behind its blocked cardinal neighbors
                CellCoord::new(2, 3),
                CellCoord::new(1, 2),
                CellCoord::new(2, 1),
            ],
        )
        .expect("at least one interaction cell should be reachable");

    assert_eq!(selected.target(), CellCoord::new(2, 1));
    assert_eq!(selected.distance(), 1);
}

#[test]
fn interaction_cells_cover_points_blocking_exteriors_and_walkable_footprints() {
    let mut world = navigation_world(5, 5);
    set_resource_node(&mut world, CellCoord::new(2, 1), true);
    let navigation = NavigationSnapshot::from_world(&world).expect("snapshot should exist");

    assert_eq!(
        navigation.point_interaction_cells(CellCoord::new(2, 2)),
        vec![
            CellCoord::new(1, 2),
            CellCoord::new(3, 2),
            CellCoord::new(2, 3),
        ]
    );

    let footprint = BuildingFootprint::new(CellCoord::new(1, 1), 2, 2);
    assert_eq!(
        navigation.exterior_interaction_cells(footprint),
        vec![
            CellCoord::new(1, 0),
            CellCoord::new(2, 0),
            CellCoord::new(0, 1),
            CellCoord::new(3, 1),
            CellCoord::new(0, 2),
            CellCoord::new(3, 2),
            CellCoord::new(1, 3),
            CellCoord::new(2, 3),
        ]
    );
    assert_eq!(
        navigation.footprint_interaction_cells(footprint),
        vec![
            CellCoord::new(1, 1),
            CellCoord::new(1, 2),
            CellCoord::new(2, 2),
        ]
    );
}

#[test]
fn route_driver_feeds_cardinal_waypoints_and_replans_after_collision_changes() {
    let mut world = navigation_world(5, 3);
    set_resource_node(&mut world, CellCoord::new(2, 1), true);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcRoute::to_cell(CellCoord::new(3, 1)),
        ))
        .id();

    drive_npc_routes(&mut world);
    assert_eq!(
        world.get::<MovementTarget>(npc).map(|target| target.coord),
        Some(CellCoord::new(1, 0))
    );
    assert_eq!(
        world
            .get::<NpcRoute>(npc)
            .expect("route should remain queued")
            .destination(),
        Some(CellCoord::new(3, 1))
    );

    set_resource_node(&mut world, CellCoord::new(2, 1), false);
    drive_npc_routes(&mut world);

    assert_eq!(
        world.get::<MovementTarget>(npc).map(|target| target.coord),
        Some(CellCoord::new(2, 1))
    );
}

#[test]
fn route_driver_does_not_modify_legacy_direct_targets() {
    let mut world = navigation_world(3, 3);
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 1)),
            MovementTarget::new(CellCoord::new(2, 2)),
        ))
        .id();

    drive_npc_routes(&mut world);

    assert_eq!(
        world.get::<MovementTarget>(npc).map(|target| target.coord),
        Some(CellCoord::new(2, 2))
    );
}

#[test]
fn unreachable_route_waits_and_replans_when_a_path_opens() {
    let mut world = navigation_world(3, 3);
    for y in 0..3 {
        set_resource_node(&mut world, CellCoord::new(1, y), true);
    }
    let npc = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(0, 1)),
            NpcRoute::to_cell(CellCoord::new(2, 1)),
        ))
        .id();

    drive_npc_routes(&mut world);
    assert!(world.get::<MovementTarget>(npc).is_none());
    assert!(world
        .get::<NpcRoute>(npc)
        .expect("blocked request should be retained")
        .is_blocked());

    set_resource_node(&mut world, CellCoord::new(1, 1), false);
    drive_npc_routes(&mut world);
    assert_eq!(
        world.get::<MovementTarget>(npc).map(|target| target.coord),
        Some(CellCoord::new(1, 1))
    );
}

fn navigation_world(width: usize, height: usize) -> World {
    let size = GridSize::new(width, height);
    let mut world = World::new();
    world.insert_resource(Grid::new(width, height));
    let mut index = TileIndex::new(size);
    for coord in size.iter_coords() {
        let entity = world
            .spawn(TileBundle::new_with_terrain(coord, TerrainKind::Grass))
            .id();
        assert!(index.set(coord, entity));
    }
    world.insert_resource(index);
    world
}

fn set_resource_node(world: &mut World, coord: CellCoord, blocked: bool) {
    let entity = world
        .resource::<TileIndex>()
        .get(coord)
        .expect("test tile should be indexed");
    if blocked {
        world.entity_mut(entity).insert(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 1,
        });
    } else {
        world.entity_mut(entity).remove::<ResourceNode>();
    }
}

fn cardinal_distance(left: CellCoord, right: CellCoord) -> i32 {
    (left.x() - right.x()).abs() + (left.y() - right.y()).abs()
}
