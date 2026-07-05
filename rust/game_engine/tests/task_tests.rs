use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingBlueprintKind, BuildingFootprint, ConstructionProgress,
};
use game_engine::grid::{CellCoord, GridSize};
use game_engine::resources::ResourceAmounts;
use game_engine::simulation::{GameSimulation, SurfaceId};
use game_engine::tasks::{maintain_construction_tasks, Task, TaskKind};

#[test]
fn test_blueprint_does_not_create_task_before_tick() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));

    simulation
        .place_building_blueprint(
            surface,
            BuildingBlueprintKind::Warehouse,
            CellCoord::new(0, 0),
        )
        .expect("warehouse should place");

    assert_eq!(construction_tasks(&simulation, surface).len(), 0);
}

#[test]
fn test_tick_creates_construction_task_for_blueprint() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let blueprint = simulation
        .place_building_blueprint(
            surface,
            BuildingBlueprintKind::Warehouse,
            CellCoord::new(0, 0),
        )
        .expect("warehouse should place");

    simulation.tick(1.0 / 60.0);

    let tasks = construction_tasks(&simulation, surface);
    assert_eq!(tasks, vec![blueprint]);
}

#[test]
fn test_repeated_ticks_do_not_duplicate_construction_tasks() {
    let mut simulation = GameSimulation::new();
    let surface = simulation.create_surface(GridSize::new(4, 4));
    let blueprint = simulation
        .place_building_blueprint(
            surface,
            BuildingBlueprintKind::Warehouse,
            CellCoord::new(0, 0),
        )
        .expect("warehouse should place");

    simulation.tick(1.0 / 60.0);
    simulation.tick(1.0 / 60.0);

    let tasks = construction_tasks(&simulation, surface);
    assert_eq!(tasks, vec![blueprint]);
}

#[test]
fn test_stale_construction_task_is_removed() {
    let mut world = World::new();
    let blueprint = spawn_blueprint(&mut world);
    let stale_task = world
        .spawn(Task::progress_building_construction(blueprint))
        .id();
    world.despawn(blueprint);

    world
        .run_system_once(maintain_construction_tasks)
        .expect("task maintenance system should run");

    assert!(world.get::<Task>(stale_task).is_none());
}

#[test]
fn test_construction_tasks_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new();
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(4, 4));
    let blueprint = simulation
        .place_building_blueprint(
            second_surface,
            BuildingBlueprintKind::Warehouse,
            CellCoord::new(0, 0),
        )
        .expect("warehouse should place");

    simulation.tick(1.0 / 60.0);

    assert_eq!(construction_tasks(&simulation, default_surface).len(), 0);
    assert_eq!(
        construction_tasks(&simulation, second_surface),
        vec![blueprint]
    );
}

fn spawn_blueprint(world: &mut World) -> Entity {
    world
        .spawn((
            Building {
                kind: BuildingBlueprintKind::Warehouse,
            },
            BuildingBlueprint,
            BuildingFootprint::new(CellCoord::new(0, 0), 2, 2),
            ConstructionProgress::new(ResourceAmounts::zero()),
        ))
        .id()
}

fn construction_tasks(simulation: &GameSimulation, surface: SurfaceId) -> Vec<Entity> {
    let mut tasks = simulation.with_surface_world(surface, |world| {
        world
            .try_query::<&Task>()
            .map(|mut query| {
                query
                    .iter(world)
                    .map(|task| match task.kind() {
                        TaskKind::ProgressBuildingConstruction { blueprint } => blueprint,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    tasks.sort_by_key(|entity| entity.to_bits());
    tasks
}
