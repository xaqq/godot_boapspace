use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::buildings::{BuildingBlueprintBundle, BuildingFootprint, BuildingKind};
use game_engine::grid::{CellCoord, GridSize};
use game_engine::simulation::{GameSimulation, SurfaceId};
use game_engine::tasks::{
    maintain_construction_tasks, ProgressBuildingConstruction,
    ProgressBuildingConstructionTaskBundle, Task,
};

#[test]
fn test_blueprint_does_not_create_task_before_tick() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));

    simulation
        .place_building_blueprint(surface, BuildingKind::Warehouse, CellCoord::new(0, 0))
        .expect("warehouse should place");

    assert_eq!(construction_tasks(&simulation, surface).len(), 0);
}

#[test]
fn test_tick_creates_construction_task_for_blueprint() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let blueprint = simulation
        .place_building_blueprint(surface, BuildingKind::Warehouse, CellCoord::new(0, 0))
        .expect("warehouse should place");

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
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let blueprint = simulation
        .place_building_blueprint(surface, BuildingKind::Warehouse, CellCoord::new(0, 0))
        .expect("warehouse should place");

    simulation.pause();
    simulation.tick();

    assert!(construction_tasks(&simulation, surface).is_empty());

    simulation.play();
    simulation.tick();

    assert_eq!(construction_tasks(&simulation, surface), vec![blueprint]);
}

#[test]
fn test_repeated_ticks_do_not_duplicate_construction_tasks() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let blueprint = simulation
        .place_building_blueprint(surface, BuildingKind::Warehouse, CellCoord::new(0, 0))
        .expect("warehouse should place");

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
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(4, 4));
    let blueprint = simulation
        .place_building_blueprint(
            second_surface,
            BuildingKind::Warehouse,
            CellCoord::new(0, 0),
        )
        .expect("warehouse should place");

    simulation.tick();

    assert_eq!(construction_tasks(&simulation, default_surface).len(), 0);
    assert_eq!(
        construction_tasks(&simulation, second_surface),
        vec![blueprint]
    );
}

fn spawn_blueprint(world: &mut World) -> Entity {
    world
        .spawn(BuildingBlueprintBundle::new(
            BuildingKind::Warehouse,
            BuildingFootprint::new(CellCoord::new(0, 0), 2, 2),
        ))
        .id()
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
