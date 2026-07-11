use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::buildings::{Building, BuildingFootprint, BuildingKind};
use game_engine::components::{
    AiSearchForFood, CarriedResource, MaxVelocity, MovementFacing, Npc, NpcPosition, Velocity,
};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::logistics::AiWheelbarrowRecovery;
use game_engine::refining::{
    assign_refining_work, maintain_refining_tasks, recipes_for_building, refinery_status,
    route_and_advance_refining_work, AiRefineResource, RecipeKind, RefineryBlockedReason,
    RefineryInventory, RefineryProduction, ReservationLedger, REFINERY_INPUT_CAPACITY,
    REFINERY_OUTPUT_CAPACITY, REFINING_TICKS_PER_UNIT,
};
use game_engine::resources::ResourceKind;
use game_engine::skills::{NpcSkills, Sawyer, SkillKind};
use game_engine::tile::{TileBundle, TileIndex};

#[test]
fn recipe_metadata_matches_the_refinement_brief() {
    let expected = [
        (
            RecipeKind::SawWood,
            BuildingKind::Sawmill,
            ResourceKind::Wood,
            ResourceKind::Planks,
            SkillKind::Sawyer,
        ),
        (
            RecipeKind::CutStone,
            BuildingKind::Stoneworks,
            ResourceKind::Stone,
            ResourceKind::StoneBlocks,
            SkillKind::Stonemason,
        ),
        (
            RecipeKind::CookCrops,
            BuildingKind::Kitchen,
            ResourceKind::Crops,
            ResourceKind::Food,
            SkillKind::Cook,
        ),
        (
            RecipeKind::CookWildBerries,
            BuildingKind::Kitchen,
            ResourceKind::WildBerries,
            ResourceKind::Food,
            SkillKind::Cook,
        ),
    ];
    for (recipe, building, input, output, skill) in expected {
        let definition = recipe.definition();
        assert_eq!(definition.building(), building);
        assert_eq!(definition.input(), input);
        assert_eq!(definition.output(), output);
        assert_eq!(definition.skill(), skill);
        assert_eq!(definition.duration_ticks(), 60);
    }
    assert_eq!(
        recipes_for_building(BuildingKind::Kitchen),
        &[RecipeKind::CookCrops, RecipeKind::CookWildBerries]
    );
}

#[test]
fn refinery_buffers_are_separate_and_capacity_limited() {
    let mut inventory = RefineryInventory::empty();
    assert_eq!(inventory.input_capacity(), REFINERY_INPUT_CAPACITY);
    assert_eq!(inventory.output_capacity(), REFINERY_OUTPUT_CAPACITY);
    assert!(inventory.add_input(BuildingKind::Kitchen, ResourceKind::Crops, 100));
    assert!(inventory.add_output(BuildingKind::Kitchen, ResourceKind::Food, 100));
    assert!(!inventory.add_input(BuildingKind::Kitchen, ResourceKind::WildBerries, 1));
    assert!(!inventory.add_output(BuildingKind::Kitchen, ResourceKind::Food, 1));
    assert_eq!(inventory.input_contents().get(ResourceKind::Crops), 100);
    assert_eq!(inventory.output_contents().get(ResourceKind::Food), 100);
}

#[test]
fn eligible_worker_completes_exactly_one_buffered_batch_in_sixty_ticks() {
    let mut world = navigation_world();
    let refinery = world
        .spawn((
            Building::new(
                BuildingKind::Sawmill,
                BuildingFootprint::new(CellCoord::new(3, 3), 2, 2),
            ),
            RefineryInventory::empty(),
            RefineryProduction::default(),
        ))
        .id();
    assert!(world
        .get_mut::<RefineryInventory>(refinery)
        .unwrap()
        .add_input(BuildingKind::Sawmill, ResourceKind::Wood, 1));
    let worker = world
        .spawn((
            Npc,
            Sawyer,
            NpcPosition::new(CellCoord::new(2, 3)),
            CarriedResource::empty(),
            NpcSkills::default(),
            Velocity::ZERO,
            MaxVelocity::default(),
            MovementFacing::default(),
        ))
        .id();

    world.run_system_once(maintain_refining_tasks).unwrap();
    assign_refining_work(&mut world);
    assert_eq!(
        world.get::<AiRefineResource>(worker).unwrap().refinery(),
        refinery
    );

    route_and_advance_refining_work(&mut world); // consumes buffered input
    for _ in 0..REFINING_TICKS_PER_UNIT {
        route_and_advance_refining_work(&mut world);
    }

    let inventory = world.get::<RefineryInventory>(refinery).unwrap();
    assert_eq!(inventory.input_contents().get(ResourceKind::Wood), 0);
    assert_eq!(inventory.output_contents().get(ResourceKind::Planks), 1);
    assert_eq!(
        world
            .get::<NpcSkills>(worker)
            .unwrap()
            .value(SkillKind::Sawyer),
        1
    );
    assert!(world.get::<AiRefineResource>(worker).is_none());
}

#[test]
fn refining_does_not_claim_a_worker_recovering_a_wheelbarrow() {
    let mut world = navigation_world();
    let refinery = spawn_sawmill_with_input(&mut world, 1);
    let worker = spawn_sawyer(&mut world, CellCoord::new(2, 3));
    world
        .entity_mut(worker)
        .insert(AiWheelbarrowRecovery::default());
    world.run_system_once(maintain_refining_tasks).unwrap();

    assign_refining_work(&mut world);

    assert!(world.get::<AiRefineResource>(worker).is_none());
    assert_eq!(
        world
            .get::<RefineryProduction>(refinery)
            .unwrap()
            .assigned_worker(),
        None
    );
    assert!(world.resource::<ReservationLedger>().claims().is_empty());
}

#[test]
fn status_prioritizes_output_full_and_reports_kitchen_recipes() {
    let mut world = navigation_world();
    let kitchen = world
        .spawn((
            Building::new(
                BuildingKind::Kitchen,
                BuildingFootprint::new(CellCoord::new(3, 3), 2, 2),
            ),
            RefineryInventory::empty(),
            RefineryProduction::default(),
        ))
        .id();
    assert!(world
        .get_mut::<RefineryInventory>(kitchen)
        .unwrap()
        .add_output(BuildingKind::Kitchen, ResourceKind::Food, 100));

    let status = refinery_status(&world, kitchen).unwrap();
    assert_eq!(
        status.supported_recipes,
        vec![RecipeKind::CookCrops, RecipeKind::CookWildBerries]
    );
    assert_eq!(
        status.blocked_reason,
        Some(RefineryBlockedReason::OutputFull)
    );
}

#[test]
fn hunger_interruption_preserves_consumed_input_and_progress_for_another_worker() {
    let mut world = navigation_world();
    let refinery = spawn_sawmill_with_input(&mut world, 1);
    let first = spawn_sawyer(&mut world, CellCoord::new(2, 3));
    let second = spawn_sawyer(&mut world, CellCoord::new(2, 4));
    world.run_system_once(maintain_refining_tasks).unwrap();
    assign_refining_work(&mut world);
    let assigned_first = world
        .get::<RefineryProduction>(refinery)
        .unwrap()
        .assigned_worker()
        .unwrap();
    let replacement = if assigned_first == first {
        second
    } else {
        first
    };
    route_and_advance_refining_work(&mut world);
    for _ in 0..10 {
        route_and_advance_refining_work(&mut world);
    }
    let progress = world
        .get::<RefineryProduction>(refinery)
        .unwrap()
        .progress_ticks();
    assert_eq!(progress, 10);

    world.entity_mut(assigned_first).insert(AiSearchForFood);
    assign_refining_work(&mut world);
    assert!(world.get::<AiRefineResource>(assigned_first).is_none());
    assert_eq!(
        world
            .get::<AiRefineResource>(replacement)
            .unwrap()
            .refinery(),
        refinery
    );
    assert_eq!(
        world
            .get::<RefineryProduction>(refinery)
            .unwrap()
            .progress_ticks(),
        10
    );

    route_and_advance_refining_work(&mut world);
    for _ in 10..REFINING_TICKS_PER_UNIT {
        route_and_advance_refining_work(&mut world);
    }
    assert_eq!(
        world
            .get::<RefineryInventory>(refinery)
            .unwrap()
            .output_contents()
            .get(ResourceKind::Planks),
        1
    );
    assert_eq!(
        world
            .get::<NpcSkills>(assigned_first)
            .unwrap()
            .value(SkillKind::Sawyer),
        0
    );
    assert_eq!(
        world
            .get::<NpcSkills>(replacement)
            .unwrap()
            .value(SkillKind::Sawyer),
        1
    );
}

#[test]
fn completed_batch_waits_at_sixty_when_output_capacity_disappears() {
    let mut world = navigation_world();
    let refinery = spawn_sawmill_with_input(&mut world, 1);
    let worker = spawn_sawyer(&mut world, CellCoord::new(2, 3));
    world.run_system_once(maintain_refining_tasks).unwrap();
    assign_refining_work(&mut world);
    route_and_advance_refining_work(&mut world);
    for _ in 0..59 {
        route_and_advance_refining_work(&mut world);
    }
    assert!(world
        .get_mut::<RefineryInventory>(refinery)
        .unwrap()
        .add_output(BuildingKind::Sawmill, ResourceKind::Planks, 100));

    route_and_advance_refining_work(&mut world);
    assert_eq!(
        world
            .get::<RefineryProduction>(refinery)
            .unwrap()
            .progress_ticks(),
        60
    );
    assert!(world.get::<AiRefineResource>(worker).is_some());
    assert_eq!(
        world
            .get::<NpcSkills>(worker)
            .unwrap()
            .value(SkillKind::Sawyer),
        0
    );

    assert!(world
        .get_mut::<RefineryInventory>(refinery)
        .unwrap()
        .consume_output(ResourceKind::Planks, 1));
    route_and_advance_refining_work(&mut world);
    assert!(world.get::<AiRefineResource>(worker).is_none());
    assert_eq!(
        world
            .get::<RefineryInventory>(refinery)
            .unwrap()
            .output_contents()
            .get(ResourceKind::Planks),
        100
    );
    assert_eq!(
        world
            .get::<NpcSkills>(worker)
            .unwrap()
            .value(SkillKind::Sawyer),
        1
    );
}

fn spawn_sawmill_with_input(world: &mut World, wood: u32) -> Entity {
    let refinery = world
        .spawn((
            Building::new(
                BuildingKind::Sawmill,
                BuildingFootprint::new(CellCoord::new(3, 3), 2, 2),
            ),
            RefineryInventory::empty(),
            RefineryProduction::default(),
        ))
        .id();
    assert!(world
        .get_mut::<RefineryInventory>(refinery)
        .unwrap()
        .add_input(BuildingKind::Sawmill, ResourceKind::Wood, wood));
    refinery
}

fn spawn_sawyer(world: &mut World, coord: CellCoord) -> Entity {
    world
        .spawn((
            Npc,
            Sawyer,
            NpcPosition::new(coord),
            CarriedResource::default(),
            NpcSkills::default(),
            Velocity::ZERO,
            MaxVelocity::default(),
            MovementFacing::default(),
        ))
        .id()
}

fn navigation_world() -> World {
    let size = GridSize::new(8, 8);
    let mut world = World::new();
    world.insert_resource(Grid::new(size.width(), size.height()));
    world.insert_resource(ReservationLedger::default());
    let mut index = TileIndex::new(size);
    for coord in size.iter_coords() {
        let tile = world.spawn(TileBundle::new(coord)).id();
        assert!(index.set(coord, tile));
    }
    world.insert_resource(index);
    world
}
