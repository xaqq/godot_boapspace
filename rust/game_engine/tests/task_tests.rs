use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::ai::AiConstructBuilding;
use game_engine::buildings::{
    BuildingBlueprint, BuildingBlueprintBundle, BuildingFootprint, BuildingKind,
    ConstructionProgress,
};
use game_engine::components::{Npc, NpcPosition, TerrainKind};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::roads::{RoadBlueprint, RoadTier};
use game_engine::simulation::{GameSimulation, SurfaceId};
use game_engine::tasks::{
    maintain_construction_tasks, manage_construction_labor, AiConstructionLabor,
    ProgressBuildingConstruction, ProgressBuildingConstructionTaskBundle, Task,
};
use game_engine::tile::{TileBundle, TileIndex};

const TEST_GENERATION_SEED: u64 = 0x5eed_cafe_f00d_beef;

#[test]
fn test_blueprint_does_not_create_task_before_tick() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(16, 16));
    let origin = first_valid_depot_origin(&simulation, surface);

    simulation
        .place_building_blueprint(surface, BuildingKind::Depot, origin)
        .expect("depot should place");

    assert_eq!(construction_tasks(&simulation, surface).len(), 0);
}

#[test]
fn test_tick_creates_construction_task_for_blueprint() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(16, 16));
    let origin = first_valid_depot_origin(&simulation, surface);
    let blueprint = simulation
        .place_building_blueprint(surface, BuildingKind::Depot, origin)
        .expect("depot should place");

    simulation.tick();

    let tasks = construction_tasks(&simulation, surface);
    assert_eq!(tasks, vec![blueprint]);
    simulation.with_surface_world(surface, |world| {
        let mut query = world
            .try_query::<(Entity, &ProgressBuildingConstruction)>()
            .expect("construction task query should be valid");
        let task_entities = query
            .iter(world)
            .map(|(entity, task)| {
                assert_eq!(task.blueprint(), blueprint);
                entity
            })
            .collect::<Vec<_>>();

        assert_eq!(task_entities.len(), 1);
        assert!(world.get::<Task>(task_entities[0]).is_some());
    });
}

#[test]
fn test_paused_tick_does_not_run_surface_schedule() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(16, 16));
    let origin = first_valid_depot_origin(&simulation, surface);
    let blueprint = simulation
        .place_building_blueprint(surface, BuildingKind::Depot, origin)
        .expect("depot should place");

    simulation.pause();
    simulation.tick();

    assert!(construction_tasks(&simulation, surface).is_empty());

    simulation.play();
    simulation.tick();

    assert_eq!(construction_tasks(&simulation, surface), vec![blueprint]);
}

#[test]
fn test_repeated_ticks_do_not_duplicate_construction_tasks() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(16, 16));
    let origin = first_valid_depot_origin(&simulation, surface);
    let blueprint = simulation
        .place_building_blueprint(surface, BuildingKind::Depot, origin)
        .expect("depot should place");

    simulation.tick();
    simulation.tick();

    let tasks = construction_tasks(&simulation, surface);
    assert_eq!(tasks, vec![blueprint]);
}

#[test]
fn test_stale_construction_task_is_removed() {
    let mut world = World::new();
    let blueprint = spawn_blueprint(&mut world);
    let stale_task = world
        .spawn(ProgressBuildingConstructionTaskBundle::new(blueprint))
        .id();
    world.despawn(blueprint);

    world
        .run_system_once(maintain_construction_tasks)
        .expect("task maintenance system should run");

    assert!(world.get::<Task>(stale_task).is_none());
    assert!(world
        .get::<ProgressBuildingConstruction>(stale_task)
        .is_none());
}

#[test]
fn test_construction_tasks_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(16, 16));
    let origin = first_valid_depot_origin(&simulation, second_surface);
    let blueprint = simulation
        .place_building_blueprint(second_surface, BuildingKind::Depot, origin)
        .expect("depot should place");

    simulation.tick();

    assert_eq!(construction_tasks(&simulation, default_surface).len(), 0);
    assert_eq!(
        construction_tasks(&simulation, second_surface),
        vec![blueprint]
    );
}

#[test]
fn multiple_workers_advance_labor_from_distinct_interaction_cells() {
    let mut world = world_with_tiles(6, 6);
    let blueprint = spawn_material_complete_blueprint(
        &mut world,
        BuildingKind::Depot,
        BuildingFootprint::new(CellCoord::new(2, 2), 2, 2),
        100,
    );
    let worker_cells = [
        CellCoord::new(2, 1),
        CellCoord::new(3, 1),
        CellCoord::new(1, 2),
        CellCoord::new(4, 2),
        CellCoord::new(1, 3),
        CellCoord::new(4, 3),
        CellCoord::new(2, 4),
        CellCoord::new(3, 4),
        CellCoord::new(0, 0),
        CellCoord::new(5, 5),
    ];
    for cell in worker_cells {
        world.spawn((Npc, NpcPosition::new(cell)));
    }

    manage_construction_labor(&mut world);

    let assignments = labor_assignments(&mut world);
    assert_eq!(assignments.len(), 8);
    let assigned_cells = assignments
        .iter()
        .map(|(_, labor)| labor.interaction_cell())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(assigned_cells.len(), 8);
    assert!(assignments
        .iter()
        .all(|(_, labor)| labor.site() == blueprint));
    for (worker, labor) in &assignments {
        *world.get_mut::<NpcPosition>(*worker).unwrap() =
            NpcPosition::new(labor.interaction_cell());
    }

    manage_construction_labor(&mut world);

    assert_eq!(
        world
            .get::<ConstructionProgress>(blueprint)
            .unwrap()
            .labor_completed(),
        8
    );
}

#[test]
fn workers_are_released_when_concurrent_labor_completes_the_site() {
    let mut world = world_with_tiles(5, 5);
    let blueprint = spawn_material_complete_blueprint(
        &mut world,
        BuildingKind::Depot,
        BuildingFootprint::new(CellCoord::new(2, 2), 1, 1),
        2,
    );
    for cell in [
        CellCoord::new(2, 1),
        CellCoord::new(1, 2),
        CellCoord::new(3, 2),
    ] {
        world.spawn((Npc, NpcPosition::new(cell)));
    }

    manage_construction_labor(&mut world);
    assert_eq!(labor_assignments(&mut world).len(), 3);

    manage_construction_labor(&mut world);

    assert_eq!(
        world
            .get::<ConstructionProgress>(blueprint)
            .unwrap()
            .labor_completed(),
        2
    );
    assert!(labor_assignments(&mut world).is_empty());
    let mut construction_query = world.query::<&AiConstructBuilding>();
    assert_eq!(construction_query.iter(&world).count(), 0);
}

#[test]
fn road_construction_has_one_labor_slot() {
    let mut world = world_with_tiles(3, 3);
    let road = world
        .spawn((
            RoadBlueprint {
                coord: CellCoord::new(1, 1),
                target_tier: RoadTier::DirtPath,
            },
            ConstructionProgress::new(RoadTier::DirtPath.material_cost()).with_required_labor(10),
        ))
        .id();
    for cell in [CellCoord::new(0, 1), CellCoord::new(2, 1)] {
        world.spawn((Npc, NpcPosition::new(cell)));
    }

    manage_construction_labor(&mut world);

    let assignments = labor_assignments(&mut world);
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].1.site(), road);
    assert_eq!(assignments[0].1.interaction_cell(), CellCoord::new(1, 1));
}

fn spawn_blueprint(world: &mut World) -> Entity {
    world
        .spawn(BuildingBlueprintBundle::new(
            BuildingKind::Depot,
            BuildingFootprint::new(CellCoord::new(0, 0), 2, 2),
        ))
        .id()
}

fn spawn_material_complete_blueprint(
    world: &mut World,
    kind: BuildingKind,
    footprint: BuildingFootprint,
    labor_required: u32,
) -> Entity {
    let cost = kind.definition().construction_cost();
    world
        .spawn((
            BuildingBlueprint { kind, footprint },
            ConstructionProgress::new(cost).with_required_labor(labor_required),
        ))
        .id()
}

fn world_with_tiles(width: usize, height: usize) -> World {
    let mut world = World::new();
    let grid = Grid::new(width, height);
    let mut tile_index = TileIndex::new(grid.size());
    for coord in grid.size().iter_coords() {
        let tile = world
            .spawn(TileBundle::new_with_terrain(coord, TerrainKind::Grass))
            .id();
        assert!(tile_index.set(coord, tile));
    }
    world.insert_resource(grid);
    world.insert_resource(tile_index);
    world
}

fn labor_assignments(world: &mut World) -> Vec<(Entity, AiConstructionLabor)> {
    let mut query = world.query::<(Entity, &AiConstructionLabor)>();
    let mut assignments = query
        .iter(world)
        .map(|(worker, labor)| (worker, *labor))
        .collect::<Vec<_>>();
    assignments.sort_unstable_by_key(|(worker, _)| worker.to_bits());
    assignments
}

fn construction_tasks(simulation: &GameSimulation, surface: SurfaceId) -> Vec<Entity> {
    let mut tasks = simulation.with_surface_world(surface, |world| {
        world
            .try_query::<&ProgressBuildingConstruction>()
            .map(|mut query| {
                query
                    .iter(world)
                    .map(|task| task.blueprint())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    tasks.sort_by_key(|entity| entity.to_bits());
    tasks
}

fn first_valid_depot_origin(simulation: &GameSimulation, surface: SurfaceId) -> CellCoord {
    simulation
        .grid_size(surface)
        .iter_coords()
        .find(|&coord| {
            simulation
                .validate_building_blueprint_placement(surface, BuildingKind::Depot, coord)
                .is_ok()
        })
        .expect("test surface should contain a valid depot origin")
}
