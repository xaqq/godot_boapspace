use bevy_ecs::prelude::*;
use game_engine::buildings::{
    BuildingBlueprintBundle, BuildingFootprint, BuildingKind, ConstructionProgress,
};
use game_engine::components::TerrainKind;
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::navigation::NavigationSnapshot;
use game_engine::navigation::{current_navigation_snapshot, refresh_navigation_snapshot};
use game_engine::resources::ResourceKind;
use game_engine::roads::{
    complete_road_construction, place_road_blueprints, road_cell_view,
    validate_road_placement_batch, Road, RoadMap, RoadPlacementError, RoadTier,
};
use game_engine::tile::{TileBundle, TileIndex};

#[test]
fn road_tiers_define_exact_costs_speeds_and_weights() {
    assert_eq!(RoadTier::DirtPath.movement_ratio(), (3, 2));
    assert_eq!(RoadTier::Cobblestone.movement_ratio(), (2, 1));
    assert_eq!(RoadTier::Flagstone.movement_ratio(), (3, 1));
    assert_eq!(RoadTier::ALL.map(RoadTier::traversal_weight), [4, 3, 2]);
    assert_eq!(
        RoadTier::Cobblestone
            .material_cost()
            .get(ResourceKind::Stone),
        1
    );
    assert_eq!(
        RoadTier::Flagstone
            .material_cost()
            .get(ResourceKind::StoneBlocks),
        1
    );
}

#[test]
fn batch_validation_is_atomic_and_preserves_first_occurrence_order() {
    let mut world = road_world(4, 2);
    world.spawn(BuildingBlueprintBundle::new(
        BuildingKind::SmallHouse,
        BuildingFootprint::new(CellCoord::new(2, 0), 1, 1),
    ));
    let coords = [
        CellCoord::new(0, 0),
        CellCoord::new(1, 0),
        CellCoord::new(0, 0),
        CellCoord::new(2, 0),
    ];
    let validation = validate_road_placement_batch(&world, RoadTier::DirtPath, coords);
    assert_eq!(
        validation
            .cells
            .iter()
            .map(|cell| cell.coord)
            .collect::<Vec<_>>(),
        vec![
            CellCoord::new(0, 0),
            CellCoord::new(1, 0),
            CellCoord::new(2, 0)
        ]
    );
    assert!(validation.cells[2]
        .errors
        .contains(&RoadPlacementError::OverlapsBuildingOrPlot));
    assert!(place_road_blueprints(&mut world, RoadTier::DirtPath, coords).is_err());
    assert!(road_cell_view(&world, CellCoord::new(0, 0)).is_none());
}

#[test]
fn upgrades_keep_the_old_tier_until_labor_finishes() {
    let mut world = road_world(2, 1);
    let entity =
        place_road_blueprints(&mut world, RoadTier::DirtPath, [CellCoord::new(0, 0)]).unwrap()[0];
    finish_labor(&mut world, entity);
    complete_road_construction(&mut world);
    assert_eq!(world.get::<Road>(entity).unwrap().tier, RoadTier::DirtPath);

    place_road_blueprints(&mut world, RoadTier::Cobblestone, [CellCoord::new(0, 0)]).unwrap();
    let pending = road_cell_view(&world, CellCoord::new(0, 0)).unwrap();
    assert_eq!(pending.completed_tier, Some(RoadTier::DirtPath));
    assert_eq!(pending.target_tier, Some(RoadTier::Cobblestone));
    assert_eq!(pending.construction.unwrap().labor_completed(), 0);
}

#[test]
fn weighted_navigation_prefers_a_geometrically_longer_faster_road_route() {
    let mut world = road_world(5, 3);
    for coord in [
        CellCoord::new(0, 0),
        CellCoord::new(1, 0),
        CellCoord::new(2, 0),
        CellCoord::new(3, 0),
        CellCoord::new(4, 0),
        CellCoord::new(4, 1),
    ] {
        world.spawn(Road {
            coord,
            tier: RoadTier::Flagstone,
        });
    }
    let snapshot = NavigationSnapshot::from_world(&world).unwrap();
    let path = snapshot
        .shortest_path_to_any(CellCoord::new(0, 1), [CellCoord::new(4, 1)])
        .unwrap();
    assert_eq!(
        path.cells(),
        &[
            CellCoord::new(0, 1),
            CellCoord::new(1, 0),
            CellCoord::new(2, 0),
            CellCoord::new(3, 0),
            CellCoord::new(4, 1),
        ]
    );
    assert_eq!(path.distance(), 9_656);
}

#[test]
fn only_effective_road_changes_advance_navigation_revision() {
    let mut world = road_world(2, 1);
    refresh_navigation_snapshot(&mut world);
    let initial = current_navigation_snapshot(&mut world)
        .unwrap()
        .fingerprint();
    let entity =
        place_road_blueprints(&mut world, RoadTier::DirtPath, [CellCoord::new(0, 0)]).unwrap()[0];
    assert_eq!(
        current_navigation_snapshot(&mut world)
            .unwrap()
            .fingerprint(),
        initial
    );
    finish_labor(&mut world, entity);
    complete_road_construction(&mut world);
    assert_ne!(
        current_navigation_snapshot(&mut world)
            .unwrap()
            .fingerprint(),
        initial
    );
}

fn finish_labor(world: &mut World, entity: Entity) {
    let required = world
        .get::<ConstructionProgress>(entity)
        .unwrap()
        .labor_required();
    for _ in 0..required {
        world
            .get_mut::<ConstructionProgress>(entity)
            .unwrap()
            .advance_labor();
    }
}

fn road_world(width: usize, height: usize) -> World {
    let size = GridSize::new(width, height);
    let mut world = World::new();
    world.insert_resource(Grid::new(width, height));
    world.insert_resource(RoadMap::default());
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
