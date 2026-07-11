use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::ai::{system_assign_farming_work, system_route_farming_work, AiSearchForFood};
use game_engine::buildings::{
    system_complete_building_construction, Building, BuildingBlueprint, BuildingFootprint,
    BuildingKind, ConstructionProgress,
};
use game_engine::components::{MovementTarget, NpcInventory, TerrainKind};
use game_engine::farming::{
    farm_field_counts, field_crop_state, maintain_farming_tasks, place_field_blueprint,
    place_field_blueprints, system_advance_field_growth, system_harvest_fields, system_seed_fields,
    AiHarvestField, AiSeedField, FarmInventory, Farmer, FieldCrop, FieldCropState, FieldOwner,
    FieldPlacementError, HarvestField, SeedField, FARM_INVENTORY_MAX_FOOD, FIELD_GROWTH_TICKS,
    FIELD_HARVEST_TICKS, FIELD_SEEDING_TICKS, MAX_FIELDS_PER_FARM,
};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::npcs::{Npc, NpcPosition, NpcSkills, SkillKind};
use game_engine::resources::ResourceAmounts;
use game_engine::simulation::GameSimulation;
use game_engine::tasks::Task;
use game_engine::tile::{TileBundle, TileIndex};

#[test]
fn field_blueprint_placement_requires_cardinal_farm_connection() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(2, 2));

    let field = place_field_blueprint(&mut world, farm, CellCoord::new(5, 3))
        .expect("field should place cardinally adjacent to farm footprint");

    assert_eq!(
        world
            .get::<FieldOwner>(field)
            .expect("field blueprint should keep owner")
            .farm(),
        farm
    );
    assert_eq!(
        place_field_blueprint(&mut world, farm, CellCoord::new(5, 5)),
        Err(FieldPlacementError::NotConnected)
    );
}

#[test]
fn field_blueprint_placement_can_chain_from_same_farm_field() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(2, 2));
    place_field_blueprint(&mut world, farm, CellCoord::new(5, 3))
        .expect("first field should place next to farm");

    let second = place_field_blueprint(&mut world, farm, CellCoord::new(6, 3))
        .expect("second field should chain from first field");

    assert_eq!(
        world
            .get::<BuildingBlueprint>(second)
            .expect("second field should be a blueprint")
            .footprint
            .origin(),
        CellCoord::new(6, 3)
    );
}

#[test]
fn field_blueprint_placement_rejects_non_farm_owner_and_overlap() {
    let mut world = farming_world();
    let warehouse = world
        .spawn(Building::new(
            BuildingKind::Warehouse,
            BuildingFootprint::new(CellCoord::new(2, 2), 2, 2),
        ))
        .id();
    let farm = spawn_farm(&mut world, CellCoord::new(5, 5));
    place_field_blueprint(&mut world, farm, CellCoord::new(8, 6)).expect("field should place");

    assert_eq!(
        place_field_blueprint(&mut world, warehouse, CellCoord::new(4, 2)),
        Err(FieldPlacementError::OwnerNotFarm)
    );
    assert_eq!(
        place_field_blueprint(&mut world, farm, CellCoord::new(8, 6)),
        Err(FieldPlacementError::OverlapsBuilding)
    );
}

#[test]
fn batch_field_placement_deduplicates_connects_and_sorts_cells() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(5, 5));

    let result = place_field_blueprints(
        &mut world,
        farm,
        [
            CellCoord::new(5, 3),
            CellCoord::new(5, 4),
            CellCoord::new(5, 3),
            CellCoord::new(4, 4),
            CellCoord::new(9, 9),
        ],
    );

    let placed_coords = result
        .placed
        .iter()
        .map(|placed| placed.coord)
        .collect::<Vec<_>>();
    assert_eq!(
        placed_coords,
        vec![
            CellCoord::new(5, 3),
            CellCoord::new(4, 4),
            CellCoord::new(5, 4),
        ]
    );
    assert_eq!(
        result
            .rejected
            .iter()
            .map(|rejected| rejected.coord)
            .collect::<Vec<_>>(),
        vec![CellCoord::new(9, 9)]
    );
    assert_eq!(result.rejected[0].error, FieldPlacementError::NotConnected);
}

#[test]
fn field_limit_counts_blueprints_and_constructed_fields() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));

    for index in 0..MAX_FIELDS_PER_FARM {
        world.spawn((
            Building::new(
                BuildingKind::Field,
                BuildingFootprint::new(CellCoord::new(index as i32 + 3, 0), 1, 1),
            ),
            FieldOwner::new(farm),
            FieldCrop::seedable(),
        ));
    }

    assert_eq!(
        place_field_blueprint(&mut world, farm, CellCoord::new(3, 1)),
        Err(FieldPlacementError::FarmFieldLimitReached)
    );
}

#[test]
fn field_batch_limit_keeps_the_connector_instead_of_a_disconnected_field() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(5, 5));

    for index in 0..MAX_FIELDS_PER_FARM - 1 {
        world.spawn((
            Building::new(
                BuildingKind::Field,
                BuildingFootprint::new(CellCoord::new(1_000 + index as i32, 1_000), 1, 1),
            ),
            FieldOwner::new(farm),
            FieldCrop::seedable(),
        ));
    }

    let distal = CellCoord::new(5, 3);
    let connector = CellCoord::new(5, 4);
    let result = place_field_blueprints(&mut world, farm, [distal, connector]);

    assert_eq!(
        result
            .placed
            .iter()
            .map(|placed| placed.coord)
            .collect::<Vec<_>>(),
        vec![connector]
    );
    assert_eq!(result.rejected.len(), 1);
    assert_eq!(result.rejected[0].coord, distal);
    assert_eq!(
        result.rejected[0].error,
        FieldPlacementError::FarmFieldLimitReached
    );
}

#[test]
fn simulation_field_placement_is_surface_scoped() {
    let mut simulation = GameSimulation::new();
    let first = simulation.create_surface(GridSize::new(8, 8));
    let second = simulation.create_surface(GridSize::new(8, 8));
    let farm = simulation
        .place_building_blueprint(first, BuildingKind::Farm, CellCoord::new(1, 1))
        .expect("farm should place");

    assert_eq!(
        simulation.place_field_blueprint(second, farm, CellCoord::new(4, 2)),
        Err(FieldPlacementError::OwnerMissing)
    );
}

#[test]
fn completed_farm_and_field_gain_farming_components() {
    let mut world = World::new();
    let farm = spawn_blueprint_with_progress(
        &mut world,
        BuildingKind::Farm,
        CellCoord::new(0, 0),
        ResourceAmounts::new(20, 30, 0, 0),
    );
    let field = spawn_blueprint_with_progress(
        &mut world,
        BuildingKind::Field,
        CellCoord::new(3, 1),
        ResourceAmounts::new(5, 1, 0, 0),
    );
    world.entity_mut(field).insert(FieldOwner::new(farm));

    world
        .run_system_once(system_complete_building_construction)
        .expect("completion system should run");

    assert!(world.get::<FarmInventory>(farm).is_some());
    assert!(world.get::<FieldCrop>(field).is_some());
    assert_eq!(
        world
            .get::<FieldOwner>(field)
            .expect("field should retain owner")
            .farm(),
        farm
    );
}

#[test]
fn farming_task_maintenance_creates_seed_and_harvest_tasks_without_duplicates() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let seedable = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::seedable(),
    );
    let grown = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 2),
        FieldCrop::growing(FIELD_GROWTH_TICKS),
    );

    run_maintain_farming_tasks(&mut world);
    run_maintain_farming_tasks(&mut world);

    assert_eq!(seed_tasks(&mut world), vec![seedable]);
    assert_eq!(harvest_tasks(&mut world), vec![grown]);
}

#[test]
fn farming_task_maintenance_removes_harvest_task_when_inventory_is_full() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::growing(FIELD_GROWTH_TICKS),
    );

    run_maintain_farming_tasks(&mut world);
    assert_eq!(harvest_tasks(&mut world), vec![field]);

    fill_farm_inventory(&mut world, farm);
    run_maintain_farming_tasks(&mut world);

    assert!(harvest_tasks(&mut world).is_empty());
}

#[test]
fn farmer_assignment_requires_farmer_tag_and_routes_to_field() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::seedable(),
    );
    run_maintain_farming_tasks(&mut world);
    let farmer = world
        .spawn((
            Npc,
            Farmer,
            NpcPosition::new(CellCoord::new(1, 1)),
            NpcInventory::empty(),
        ))
        .id();
    let non_farmer = world
        .spawn((
            Npc,
            NpcPosition::new(CellCoord::new(1, 2)),
            NpcInventory::empty(),
        ))
        .id();

    world
        .run_system_once(system_assign_farming_work)
        .expect("assignment should run");
    world
        .run_system_once(system_route_farming_work)
        .expect("routing should run");

    assert_eq!(
        world
            .get::<AiSeedField>(farmer)
            .expect("farmer should take seed work")
            .field(),
        field
    );
    assert_eq!(
        world
            .get::<MovementTarget>(farmer)
            .expect("farmer should route to field")
            .coord,
        CellCoord::new(3, 1)
    );
    assert!(world.get::<AiSeedField>(non_farmer).is_none());
}

#[test]
fn seeding_progress_is_stored_on_field_and_awards_farmer_xp_on_completion() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::seedable(),
    );
    let npc = world
        .spawn((
            Npc,
            Farmer,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiSeedField::new(field),
        ))
        .id();

    for _ in 0..(FIELD_SEEDING_TICKS - 1) {
        run_seed_fields(&mut world);
    }

    assert_eq!(
        world
            .get::<FieldCrop>(field)
            .expect("field should have crop")
            .seeding_progress_ticks(),
        FIELD_SEEDING_TICKS - 1
    );
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);

    run_seed_fields(&mut world);

    let crop = *world
        .get::<FieldCrop>(field)
        .expect("field should have crop");
    assert_eq!(crop.seeding_progress_ticks(), FIELD_SEEDING_TICKS);
    assert_eq!(crop.growth_ticks(), Some(0));
    assert!(world.get::<AiSeedField>(npc).is_none());
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 1);
}

#[test]
fn seeding_interruption_removes_work_without_resetting_field_progress() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::with_seeding_progress(12),
    );
    let npc = world
        .spawn((
            Npc,
            Farmer,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiSeedField::new(field),
            AiSearchForFood,
        ))
        .id();

    run_seed_fields(&mut world);

    assert_eq!(
        world
            .get::<FieldCrop>(field)
            .expect("field should keep crop")
            .seeding_progress_ticks(),
        12
    );
    assert!(world.get::<AiSeedField>(npc).is_none());
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);
}

#[test]
fn crop_growth_reaches_grown_state_after_growth_duration() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::growing(FIELD_GROWTH_TICKS - 1),
    );

    world
        .run_system_once(system_advance_field_growth)
        .expect("growth system should run");

    assert_eq!(field_crop_state(&world, field), Some(FieldCropState::Grown));
}

#[test]
fn harvest_adds_food_to_farm_inventory_and_resets_field() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::growing(FIELD_GROWTH_TICKS),
    );
    let npc = world
        .spawn((
            Npc,
            Farmer,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiHarvestField::new(field),
        ))
        .id();

    for _ in 0..FIELD_HARVEST_TICKS {
        run_harvest_fields(&mut world);
    }

    assert_eq!(
        world
            .get::<FarmInventory>(farm)
            .expect("farm should have inventory")
            .food(),
        1
    );
    assert_eq!(
        *world.get::<FieldCrop>(field).unwrap(),
        FieldCrop::seedable()
    );
    assert!(world.get::<AiHarvestField>(npc).is_none());
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 1);
}

#[test]
fn harvest_interruption_removes_work_without_consuming_crop() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::growing(FIELD_GROWTH_TICKS),
    );
    let npc = world
        .spawn((
            Npc,
            Farmer,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiHarvestField::new(field),
            AiSearchForFood,
        ))
        .id();

    run_harvest_fields(&mut world);

    assert_eq!(field_crop_state(&world, field), Some(FieldCropState::Grown));
    assert_eq!(world.get::<FarmInventory>(farm).unwrap().food(), 0);
    assert!(world.get::<AiHarvestField>(npc).is_none());
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);
}

#[test]
fn full_farm_inventory_does_not_destroy_grown_crop() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    fill_farm_inventory(&mut world, farm);
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::growing(FIELD_GROWTH_TICKS),
    );
    let npc = world
        .spawn((
            Npc,
            Farmer,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiHarvestField::new(field),
        ))
        .id();

    for _ in 0..FIELD_HARVEST_TICKS {
        run_harvest_fields(&mut world);
    }

    assert_eq!(field_crop_state(&world, field), Some(FieldCropState::Grown));
    assert_eq!(
        world.get::<FarmInventory>(farm).unwrap().food(),
        FARM_INVENTORY_MAX_FOOD
    );
    assert!(world.get::<AiHarvestField>(npc).is_none());
    assert_eq!(npc_skill(&world, npc, SkillKind::Farmer), 0);
}

#[test]
fn farm_field_counts_include_blueprints_and_constructed_fields() {
    let mut world = farming_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::seedable(),
    );
    let blueprint = world
        .spawn((
            BuildingBlueprint {
                kind: BuildingKind::Field,
                footprint: BuildingFootprint::new(CellCoord::new(3, 2), 1, 1),
            },
            ConstructionProgress::new(ResourceAmounts::zero()),
            FieldOwner::new(farm),
        ))
        .id();

    assert!(world.get::<Task>(blueprint).is_none());
    assert_eq!(farm_field_counts(&world, farm), (2, 1));
}

fn farming_world() -> World {
    let size = GridSize::new(16, 16);
    let mut world = World::new();
    world.insert_resource(Grid::new(size.width(), size.height()));
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

fn spawn_farm(world: &mut World, origin: CellCoord) -> Entity {
    world
        .spawn((
            Building::new(BuildingKind::Farm, BuildingFootprint::new(origin, 3, 3)),
            FarmInventory::empty(),
        ))
        .id()
}

fn spawn_field(world: &mut World, farm: Entity, coord: CellCoord, crop: FieldCrop) -> Entity {
    world
        .spawn((
            Building::new(BuildingKind::Field, BuildingFootprint::new(coord, 1, 1)),
            FieldOwner::new(farm),
            crop,
        ))
        .id()
}

fn spawn_blueprint_with_progress(
    world: &mut World,
    kind: BuildingKind,
    origin: CellCoord,
    deposited: ResourceAmounts,
) -> Entity {
    let definition = kind.definition();
    world
        .spawn((
            BuildingBlueprint {
                kind,
                footprint: BuildingFootprint::new(origin, definition.width(), definition.height()),
            },
            ConstructionProgress::new(deposited),
        ))
        .id()
}

fn run_maintain_farming_tasks(world: &mut World) {
    world
        .run_system_once(maintain_farming_tasks)
        .expect("farming task maintenance should run");
}

fn run_seed_fields(world: &mut World) {
    world
        .run_system_once(system_seed_fields)
        .expect("seed system should run");
}

fn run_harvest_fields(world: &mut World) {
    world
        .run_system_once(system_harvest_fields)
        .expect("harvest system should run");
}

fn seed_tasks(world: &mut World) -> Vec<Entity> {
    let mut tasks = world
        .try_query::<&SeedField>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|task| task.field())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    tasks.sort_by_key(|entity| entity.to_bits());
    tasks
}

fn harvest_tasks(world: &mut World) -> Vec<Entity> {
    let mut tasks = world
        .try_query::<&HarvestField>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|task| task.field())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    tasks.sort_by_key(|entity| entity.to_bits());
    tasks
}

fn fill_farm_inventory(world: &mut World, farm: Entity) {
    let mut inventory = world
        .get_mut::<FarmInventory>(farm)
        .expect("farm should have inventory");
    for _ in 0..FARM_INVENTORY_MAX_FOOD {
        assert!(inventory.add_food(1));
    }
}

fn npc_skill(world: &World, npc: Entity, kind: SkillKind) -> u32 {
    world
        .get::<NpcSkills>(npc)
        .expect("NPC should have skills")
        .value(kind)
}
