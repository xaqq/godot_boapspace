use game_engine::components::{
    CarriedResource, FoodPouch, Terrain, TerrainKind, Tile, TilePosition, FOOD_POUCH_CAPACITY,
};
use game_engine::grid::{CellCoord, GridSize};
use game_engine::npcs::{
    BirthDate, HungerState, Npc, NpcAppearance, NpcHunger, NpcName, NpcPosition, WorldDateTime,
    INITIAL_NPC_BIRTH_DAY, INITIAL_NPC_NAME, INITIAL_NPC_SPECS,
};
use game_engine::resource_nodes::{terrain_allows_resource, ResourceNode};
use game_engine::resources::ResourceKind;
use game_engine::simulation::{
    GameSimulation, SimulationSpeed, SurfaceLookupError, DEFAULT_GRID_SIZE,
};
use game_engine::tile::TileIndex;
use game_engine::time::{SECONDS_PER_DAY, SIMULATION_TICK_SECONDS};
use std::collections::HashSet;
use std::time::Duration;

const TEST_GENERATION_SEED: u64 = 0x5eed_cafe_f00d_beef;

#[test]
fn test_new_creates_default_surface() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();

    assert_eq!(simulation.surface_count(), 1);
    assert_eq!(simulation.grid_size(surface), DEFAULT_GRID_SIZE);
}

#[test]
fn test_new_starts_world_date_time_at_epoch() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);

    assert_eq!(simulation.world_date_time(), WorldDateTime::from_day(0));
}

#[test]
fn test_create_surface_returns_distinct_id() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(10, 12));

    assert_ne!(default_surface, second_surface);
    assert_eq!(simulation.surface_count(), 2);
    assert_eq!(simulation.grid_size(second_surface), GridSize::new(10, 12));
}

#[test]
fn test_surface_id_at_returns_valid_surface_ids() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(10, 12));

    assert_eq!(simulation.surface_id_at(0), Ok(default_surface));
    assert_eq!(simulation.surface_id_at(1), Ok(second_surface));
}

#[test]
fn test_surface_id_at_rejects_invalid_indexes() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    simulation.create_surface(GridSize::new(10, 12));

    assert_eq!(
        simulation.surface_id_at(2),
        Err(SurfaceLookupError::IndexOutOfRange {
            index: 2,
            surface_count: 2,
        })
    );
}

#[test]
fn test_tile_coordinate_reads_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(4, 5));

    let default_coords = simulation.tile_coords(default_surface);
    let second_coords = simulation.tile_coords(second_surface);

    assert!(default_coords.contains(&CellCoord::new(100, 100)));
    assert!(!second_coords.contains(&CellCoord::new(100, 100)));
    assert!(second_coords.contains(&CellCoord::new(3, 4)));
}

#[test]
fn test_tile_terrain_at_returns_none_for_missing_tile() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(4, 4));

    assert_eq!(
        simulation.tile_terrain_at(surface, CellCoord::new(10, 10)),
        None
    );
}

#[test]
fn test_tick_runs_across_multiple_surfaces() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    simulation.create_surface(GridSize::new(6, 6));

    simulation.tick();

    assert_eq!(simulation.surface_count(), 2);
}

#[test]
fn test_simulation_starts_playing() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);

    assert!(simulation.is_playing());
}

#[test]
fn test_simulation_defaults_to_one_x_speed() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);

    assert_eq!(simulation.simulation_speed(), SimulationSpeed::OneX);
}

#[test]
fn test_tick_advances_world_date_time_by_fixed_duration() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let before = simulation.world_date_time();

    simulation.tick();

    assert_eq!(
        simulation.world_date_time().elapsed_since_world_epoch(),
        before.elapsed_since_world_epoch() + Duration::from_secs(SIMULATION_TICK_SECONDS)
    );
}

#[test]
fn test_two_x_speed_runs_two_fixed_ticks() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let before = simulation.world_date_time();

    simulation.set_simulation_speed(SimulationSpeed::TwoX);
    simulation.tick();

    assert_eq!(
        simulation.world_date_time().elapsed_since_world_epoch(),
        before.elapsed_since_world_epoch() + Duration::from_secs(2 * SIMULATION_TICK_SECONDS)
    );
}

#[test]
fn test_four_x_speed_runs_four_fixed_ticks() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let before = simulation.world_date_time();

    simulation.set_simulation_speed(SimulationSpeed::FourX);
    simulation.tick();

    assert_eq!(
        simulation.world_date_time().elapsed_since_world_epoch(),
        before.elapsed_since_world_epoch() + Duration::from_secs(4 * SIMULATION_TICK_SECONDS)
    );
}

#[test]
fn test_fifty_x_speed_runs_fifty_fixed_ticks() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let before = simulation.world_date_time();

    simulation.set_simulation_speed(SimulationSpeed::FiftyX);
    simulation.tick();

    assert_eq!(
        simulation.world_date_time().elapsed_since_world_epoch(),
        before.elapsed_since_world_epoch() + Duration::from_secs(50 * SIMULATION_TICK_SECONDS)
    );
}

#[test]
fn test_hundred_x_speed_runs_one_hundred_fixed_ticks() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let before = simulation.world_date_time();

    simulation.set_simulation_speed(SimulationSpeed::HundredX);
    simulation.tick();

    assert_eq!(
        simulation.world_date_time().elapsed_since_world_epoch(),
        before.elapsed_since_world_epoch() + Duration::from_secs(100 * SIMULATION_TICK_SECONDS)
    );
}

#[test]
fn test_paused_tick_does_not_advance_world_date_time() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let before = simulation.world_date_time();

    simulation.set_simulation_speed(SimulationSpeed::FourX);
    simulation.pause();
    simulation.tick();

    assert!(!simulation.is_playing());
    assert_eq!(simulation.world_date_time(), before);
}

#[test]
fn test_resume_allows_world_date_time_to_advance_again() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);

    simulation.pause();
    simulation.tick();
    let paused_date_time = simulation.world_date_time();
    simulation.play();
    simulation.tick();

    assert!(simulation.is_playing());
    assert_eq!(
        simulation.world_date_time().elapsed_since_world_epoch(),
        paused_date_time.elapsed_since_world_epoch() + Duration::from_secs(SIMULATION_TICK_SECONDS)
    );
}

#[test]
fn test_created_surface_inherits_current_world_date_time() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    simulation.tick();
    let current_date_time = simulation.world_date_time();

    let surface = simulation.create_surface(GridSize::new(4, 4));

    assert_eq!(
        surface_world_date_time(&simulation, surface),
        current_date_time
    );
}

#[test]
fn test_tick_syncs_world_date_time_across_surfaces() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(4, 4));

    simulation.tick();
    let current_date_time = simulation.world_date_time();

    assert_eq!(
        surface_world_date_time(&simulation, default_surface),
        current_date_time
    );
    assert_eq!(
        surface_world_date_time(&simulation, second_surface),
        current_date_time
    );
}

#[test]
fn test_four_x_tick_syncs_world_date_time_across_surfaces() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(4, 4));

    simulation.set_simulation_speed(SimulationSpeed::FourX);
    simulation.tick();
    let current_date_time = simulation.world_date_time();

    assert_eq!(
        surface_world_date_time(&simulation, default_surface),
        current_date_time
    );
    assert_eq!(
        surface_world_date_time(&simulation, second_surface),
        current_date_time
    );
}

#[test]
fn test_surface_spawns_one_tile_entity_per_cell() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(4, 5));

    let tiles = tiles(&simulation, surface);

    assert_eq!(tiles.len(), 20);
}

#[test]
fn test_tile_index_contains_one_entity_per_cell() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(4, 5));
    let size = simulation.grid_size(surface);
    let (indexed_size, indexed_len, indexed_coords) =
        simulation.with_surface_world(surface, |world| {
            let index = world.resource::<TileIndex>();
            (
                index.size(),
                index.len(),
                index.iter().map(|(coord, _)| coord).collect::<Vec<_>>(),
            )
        });
    let unique_tiles = indexed_coords.iter().copied().collect::<HashSet<_>>();

    assert_eq!(indexed_size, size);
    assert_eq!(
        indexed_len,
        size.cell_count().expect("grid size should fit")
    );
    assert_eq!(indexed_coords.len(), unique_tiles.len());
    for coord in indexed_coords {
        assert!(size.contains(coord), "{coord:?} should be within {size:?}");
    }
}

#[test]
fn test_tile_entities_are_unique_within_bounds_and_have_valid_terrain() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(64, 64));
    let size = simulation.grid_size(surface);
    let tiles = tiles(&simulation, surface);
    let unique_tiles = tiles
        .iter()
        .map(|(coord, _)| *coord)
        .collect::<HashSet<_>>();
    let terrain_kinds = tiles
        .iter()
        .map(|(_, terrain)| *terrain)
        .collect::<HashSet<_>>();

    assert_eq!(tiles.len(), unique_tiles.len());
    assert_eq!(
        tiles.len(),
        size.cell_count().expect("grid size should fit")
    );
    for (coord, terrain) in tiles {
        assert!(size.contains(coord), "{coord:?} should be within {size:?}");
        assert!(TerrainKind::ALL.contains(&terrain));
    }
    assert!(
        TerrainKind::ALL
            .iter()
            .all(|terrain| terrain_kinds.contains(terrain)),
        "initial terrain generation should produce every terrain kind"
    );
}

#[test]
fn test_tile_coords_are_complete_unique_and_in_bounds() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(7, 9));
    let size = simulation.grid_size(surface);
    let coords = simulation.tile_coords(surface);
    let unique_tiles = coords.iter().copied().collect::<HashSet<_>>();

    assert_eq!(
        coords.len(),
        size.cell_count().expect("grid size should fit")
    );
    assert_eq!(coords.len(), unique_tiles.len());
    for coord in coords {
        assert!(size.contains(coord), "{coord:?} should be within {size:?}");
    }
}

#[test]
fn test_default_and_created_surfaces_have_resource_nodes() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(10, 12));

    assert!(!resource_nodes(&mut simulation, default_surface).is_empty());
    assert!(!resource_nodes(&mut simulation, second_surface).is_empty());
}

#[test]
fn test_resource_nodes_are_within_bounds() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.create_surface(GridSize::new(17, 19));
    let size = simulation.grid_size(surface);

    for (coord, _, _) in resource_nodes(&mut simulation, surface) {
        assert!(size.contains(coord), "{coord:?} should be within {size:?}");
    }
}

#[test]
fn test_resource_nodes_are_attached_to_tile_entities() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();
    let (node_count, attached_count) =
        simulation.with_surface_world(surface, resource_node_attachment_counts);

    assert_ne!(node_count, 0);
    assert_eq!(node_count, attached_count);
}

#[test]
fn test_resource_node_quantities_are_within_generated_range() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();

    for (_, _, quantity) in resource_nodes(&mut simulation, surface) {
        assert!(
            (50..=150).contains(&quantity),
            "{quantity} should be within generated resource node range"
        );
    }
}

#[test]
fn test_resource_nodes_do_not_share_tiles() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();
    let nodes = resource_nodes(&mut simulation, surface);
    let unique_tiles = nodes
        .iter()
        .map(|(coord, _, _)| *coord)
        .collect::<HashSet<_>>();

    assert_eq!(nodes.len(), unique_tiles.len());
}

#[test]
fn test_resource_nodes_only_spawn_on_allowed_terrain_at_target_density() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();
    let nodes = resource_nodes(&simulation, surface);
    let expected_count = DEFAULT_GRID_SIZE
        .cell_count()
        .expect("default grid size should fit")
        * 15
        / 1_000;

    assert_eq!(nodes.len(), expected_count);
    for (coord, resource, _) in nodes {
        let terrain = simulation
            .tile_terrain_at(surface, coord)
            .expect("resource node should be attached to a terrain tile");
        assert!(
            terrain_allows_resource(terrain, resource),
            "{resource:?} should not spawn on {terrain:?} at {coord:?}"
        );
    }
}

#[test]
fn test_default_start_area_is_non_water_and_resource_free() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();
    let center = CellCoord::from_usize(
        DEFAULT_GRID_SIZE.width() / 2,
        DEFAULT_GRID_SIZE.height() / 2,
    )
    .expect("default grid center should fit");
    let resource_coords = resource_nodes(&simulation, surface)
        .into_iter()
        .map(|(coord, _, _)| coord)
        .collect::<HashSet<_>>();

    for y_offset in -1..=1 {
        for x_offset in -1..=1 {
            let coord = CellCoord::new(center.x() + x_offset, center.y() + y_offset);
            let terrain = simulation
                .tile_terrain_at(surface, coord)
                .expect("default start area should contain a terrain tile");
            assert_ne!(terrain, TerrainKind::Water);
            assert!(!resource_coords.contains(&coord));
        }
    }
}

#[test]
fn test_resource_node_generation_is_deterministic_for_same_size() {
    let mut first = GameSimulation::new(TEST_GENERATION_SEED);
    let mut second = GameSimulation::new(TEST_GENERATION_SEED);
    let first_default = first.default_surface_id();
    let second_default = second.default_surface_id();

    assert_eq!(
        sorted_resource_nodes(&mut first, first_default),
        sorted_resource_nodes(&mut second, second_default)
    );

    let first_surface = first.create_surface(GridSize::new(31, 29));
    let second_surface = second.create_surface(GridSize::new(31, 29));

    assert_eq!(
        sorted_resource_nodes(&mut first, first_surface),
        sorted_resource_nodes(&mut second, second_surface)
    );
    assert_eq!(tiles(&first, first_surface), tiles(&second, second_surface));
}

#[test]
fn test_different_generation_seeds_produce_different_maps() {
    let first = GameSimulation::new(TEST_GENERATION_SEED);
    let second = GameSimulation::new(TEST_GENERATION_SEED.wrapping_add(1));

    assert_ne!(
        tiles(&first, first.default_surface_id()),
        tiles(&second, second.default_surface_id())
    );
}

#[test]
fn test_equal_sized_surfaces_have_distinct_derived_seeds() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let first = simulation.create_surface(GridSize::new(64, 64));
    let second = simulation.create_surface(GridSize::new(64, 64));

    assert_ne!(tiles(&simulation, first), tiles(&simulation, second));
}

#[test]
fn test_resource_node_queries_are_scoped_per_surface() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(7, 9));

    assert_ne!(
        sorted_resource_nodes(&mut simulation, default_surface),
        sorted_resource_nodes(&mut simulation, second_surface)
    );
}

#[test]
fn test_tick_does_not_duplicate_resource_nodes() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();
    let before = sorted_resource_nodes(&mut simulation, surface);

    simulation.tick();
    simulation.tick();

    assert_eq!(sorted_resource_nodes(&mut simulation, surface), before);
}

#[test]
fn test_default_surface_spawns_initial_npc() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();

    assert_eq!(npcs(&simulation, surface).len(), INITIAL_NPC_SPECS.len());
}

#[test]
fn test_created_surfaces_do_not_spawn_initial_npc() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let default_surface = simulation.default_surface_id();
    let second_surface = simulation.create_surface(GridSize::new(10, 12));

    assert_eq!(
        npcs(&simulation, default_surface).len(),
        INITIAL_NPC_SPECS.len()
    );
    assert!(npcs(&simulation, second_surface).is_empty());
}

#[test]
fn test_initial_npcs_have_identity_birth_date_age_appearance_and_cluster_position() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();
    let center = CellCoord::from_usize(
        DEFAULT_GRID_SIZE.width() / 2,
        DEFAULT_GRID_SIZE.height() / 2,
    )
    .expect("default grid center should fit in CellCoord");
    let mut initial_npcs = npcs(&simulation, surface);
    initial_npcs.sort_by(|a, b| a.1.cmp(&b.1));

    assert_eq!(
        initial_npcs,
        vec![
            (
                CellCoord::new(center.x() + 1, center.y()),
                "Ilya Ren".to_string(),
                326,
                0,
                NpcAppearance::Engineer,
            ),
            (
                center,
                INITIAL_NPC_NAME.to_string(),
                INITIAL_NPC_BIRTH_DAY,
                0,
                NpcAppearance::Colonist,
            ),
            (
                CellCoord::new(center.x(), center.y() + 1),
                "Sera Nox".to_string(),
                334,
                0,
                NpcAppearance::Botanist,
            ),
            (
                CellCoord::new(center.x() - 1, center.y()),
                "Toma Kade".to_string(),
                311,
                0,
                NpcAppearance::Miner,
            ),
            (
                CellCoord::new(center.x(), center.y() - 1),
                "Vale Arin".to_string(),
                303,
                0,
                NpcAppearance::Scout,
            ),
        ]
    );
}

#[test]
fn test_initial_npc_starts_with_food_pouch_and_empty_cargo() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();

    let containers = simulation
        .with_surface_world(surface, |world| {
            let mut query = world.try_query::<(&FoodPouch, &CarriedResource, &Npc)>()?;
            Some(
                query
                    .iter(world)
                    .map(|(pouch, cargo, _)| (*pouch, *cargo))
                    .collect::<Vec<_>>(),
            )
        })
        .expect("default NPCs should have split containers");

    assert_eq!(containers.len(), INITIAL_NPC_SPECS.len());
    for (pouch, cargo) in containers {
        assert_eq!(pouch.amount(), 20);
        assert_eq!(pouch.capacity(), FOOD_POUCH_CAPACITY);
        assert_eq!(cargo.stack(), None);
    }
}

#[test]
fn test_initial_npc_starts_fed() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();

    let hunger_states = npc_hunger_states(&simulation, surface);

    assert_eq!(hunger_states.len(), INITIAL_NPC_SPECS.len());
    assert!(hunger_states
        .into_iter()
        .all(|hunger_state| hunger_state == HungerState::Fed));
}

#[test]
fn test_paused_tick_does_not_advance_npc_hunger() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();

    simulation.pause();
    tick_days(&mut simulation, 2);

    let hunger_states = npc_hunger_states(&simulation, surface);

    assert_eq!(hunger_states.len(), INITIAL_NPC_SPECS.len());
    assert!(hunger_states
        .into_iter()
        .all(|hunger_state| hunger_state == HungerState::Fed));
}

#[test]
fn test_default_npc_does_not_treat_natural_ingredients_as_cooked_food() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();

    simulation.tick();

    let hunger_states = npc_hunger_states(&simulation, surface);

    assert_eq!(hunger_states.len(), INITIAL_NPC_SPECS.len());
    assert!(hunger_states
        .into_iter()
        .all(|hunger_state| hunger_state == HungerState::Fed));
    assert!(resource_nodes(&simulation, surface)
        .into_iter()
        .all(|(_, kind, _)| kind != ResourceKind::Food));
}

fn sorted_resource_nodes(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> Vec<(CellCoord, ResourceKind, u32)> {
    let mut nodes = resource_nodes(simulation, surface);
    nodes.sort_unstable_by_key(|(coord, kind, quantity)| {
        (coord.y(), coord.x(), *kind as u8, *quantity)
    });
    nodes
}

fn resource_nodes(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> Vec<(CellCoord, ResourceKind, u32)> {
    simulation.with_surface_world(surface, query_resource_nodes)
}

fn tiles(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> Vec<(CellCoord, TerrainKind)> {
    simulation.with_surface_world(surface, query_tiles)
}

fn npcs(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> Vec<(CellCoord, String, u64, u32, NpcAppearance)> {
    simulation.with_surface_world(surface, query_npcs)
}

fn npc_hunger_states(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> Vec<HungerState> {
    simulation
        .with_surface_world(surface, |world| {
            let mut query = world.try_query::<(&NpcHunger, &Npc)>()?;
            Some(
                query
                    .iter(world)
                    .map(|(hunger, _)| hunger.state())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default()
}

fn tick_days(simulation: &mut GameSimulation, days: u64) {
    let ticks_per_day = SECONDS_PER_DAY / SIMULATION_TICK_SECONDS;
    for _ in 0..(days * ticks_per_day) {
        simulation.tick();
    }
}

fn surface_world_date_time(
    simulation: &GameSimulation,
    surface: game_engine::simulation::SurfaceId,
) -> WorldDateTime {
    simulation.with_surface_world(surface, |world| *world.resource::<WorldDateTime>())
}

fn resource_node_attachment_counts(world: &bevy_ecs::world::World) -> (usize, usize) {
    world
        .try_query::<(&ResourceNode, Option<&Tile>, Option<&TilePosition>)>()
        .map(|mut query| {
            query.iter(world).fold(
                (0, 0),
                |(node_count, attached_count), (_, tile, position)| {
                    (
                        node_count + 1,
                        attached_count + usize::from(tile.is_some() && position.is_some()),
                    )
                },
            )
        })
        .unwrap_or_default()
}

fn query_resource_nodes(world: &bevy_ecs::world::World) -> Vec<(CellCoord, ResourceKind, u32)> {
    world
        .try_query::<(&TilePosition, &ResourceNode, &Tile)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(position, node, _)| (position.coord, node.kind, node.quantity))
                .collect()
        })
        .unwrap_or_default()
}

fn query_tiles(world: &bevy_ecs::world::World) -> Vec<(CellCoord, TerrainKind)> {
    world
        .try_query::<(&TilePosition, &Terrain, &Tile)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(position, terrain, _)| (position.coord, terrain.kind))
                .collect()
        })
        .unwrap_or_default()
}

fn query_npcs(world: &bevy_ecs::world::World) -> Vec<(CellCoord, String, u64, u32, NpcAppearance)> {
    let world_date_time = *world.resource::<WorldDateTime>();

    world
        .try_query::<(&NpcPosition, &NpcName, &BirthDate, &NpcAppearance, &Npc)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(position, name, birth_date, appearance, _)| {
                    (
                        position.coord,
                        name.as_str().to_string(),
                        birth_date.elapsed_since_world_epoch().as_secs() / SECONDS_PER_DAY,
                        world_date_time.age_years_since(*birth_date),
                        *appearance,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}
