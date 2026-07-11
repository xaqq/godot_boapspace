use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use game_engine::ai::{system_assign_plot_work, AiSearchForFood};
use game_engine::buildings::{
    system_complete_building_construction, validate_building_blueprint_placement, Building,
    BuildingBlueprint, BuildingFootprint, BuildingKind, BuildingPlacementError,
    ConstructionProgress,
};
use game_engine::collision::collision_flags_at;
use game_engine::components::{NpcInventory, Terrain, TerrainKind};
use game_engine::farming::{
    maintain_farming_tasks, AiSeedField, FarmInventory, Farmer, FieldCrop, FieldOwner,
    FIELD_SEEDING_TICKS,
};
use game_engine::forestry::{
    forester_lodge_tree_plot_counts, maintain_forestry_tasks, place_tree_plot_blueprint,
    place_tree_plot_blueprints, system_advance_tree_growth, system_cut_tree_plots,
    system_seed_tree_plots, tree_plot_state, AiCutTreePlot, AiSeedTreePlot, CutTreePlot, Forester,
    ForesterLodgeInventory, SeedTreePlot, TreePlotGrowth, TreePlotOwner, TreePlotPlacementError,
    TreePlotState, FORESTER_LODGE_INVENTORY_MAX_WOOD, MAX_TREE_PLOTS_PER_FORESTER_LODGE,
    TREE_PLOT_CUTTING_TICKS, TREE_PLOT_GROWTH_TICKS, TREE_PLOT_SEEDING_TICKS,
};
use game_engine::grid::{CellCoord, Grid, GridSize};
use game_engine::npcs::{Npc, NpcPosition, NpcSkills, SkillKind};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::simulation::GameSimulation;
use game_engine::tile::{TileBundle, TileIndex};
use game_engine::time::SIMULATION_TICKS_PER_YEAR;

const TEST_GENERATION_SEED: u64 = 0x5eed_cafe_f00d_beef;

#[test]
fn forestry_building_definitions_and_durations_match_the_design() {
    let lodge = BuildingKind::ForesterLodge.definition();
    assert_eq!((lodge.width(), lodge.height()), (3, 3));
    assert_eq!(
        lodge.construction_cost(),
        ResourceAmounts::zero()
            .with(ResourceKind::Planks, 20)
            .with(ResourceKind::StoneBlocks, 30)
    );

    let plot = BuildingKind::TreePlot.definition();
    assert_eq!((plot.width(), plot.height()), (1, 1));
    assert_eq!(
        plot.construction_cost(),
        ResourceAmounts::zero()
            .with(ResourceKind::Planks, 5)
            .with(ResourceKind::StoneBlocks, 1)
    );

    assert_eq!(TREE_PLOT_SEEDING_TICKS, FIELD_SEEDING_TICKS * 5);
    assert_eq!(TREE_PLOT_SEEDING_TICKS, 7_200);
    assert_eq!(TREE_PLOT_GROWTH_TICKS, SIMULATION_TICKS_PER_YEAR * 5);
    assert_eq!(TREE_PLOT_GROWTH_TICKS, 2_628_000);
    assert_eq!(TREE_PLOT_CUTTING_TICKS, 60);
}

#[test]
fn standalone_tree_plot_placement_is_rejected() {
    let world = forestry_world();

    assert_eq!(
        validate_building_blueprint_placement(&world, BuildingKind::TreePlot, CellCoord::new(3, 1),),
        Err(BuildingPlacementError::TreePlotRequiresLodge)
    );
}

#[test]
fn tree_plot_blocks_building_but_remains_npc_walkable() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let coord = CellCoord::new(3, 1);
    spawn_tree_plot(&mut world, lodge, coord, TreePlotGrowth::seedable());

    let flags = collision_flags_at(&world, coord).expect("Tree Plot cell should be indexed");
    assert!(flags.is_build_blocked());
    assert!(!flags.is_npc_walk_blocked());
}

#[test]
fn every_initial_npc_is_a_forester() {
    let simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let surface = simulation.default_surface_id();

    let (npc_count, forester_count) = simulation.with_surface_world(surface, |world| {
        let mut query = world
            .try_query::<(&Npc, Option<&Forester>)>()
            .expect("initial NPC query should be valid");
        query
            .iter(world)
            .fold((0, 0), |(npcs, foresters), (_, role)| {
                (npcs + 1, foresters + usize::from(role.is_some()))
            })
    });

    assert!(npc_count > 0);
    assert_eq!(forester_count, npc_count);
}

#[test]
fn lodge_and_tree_plot_placement_require_only_grass() {
    for terrain in [TerrainKind::Dirt, TerrainKind::Sand, TerrainKind::Water] {
        let mut world = forestry_world();
        set_terrain(&mut world, CellCoord::new(1, 1), terrain);

        assert_eq!(
            validate_building_blueprint_placement(
                &world,
                BuildingKind::ForesterLodge,
                CellCoord::new(0, 0),
            ),
            Err(BuildingPlacementError::InvalidTerrain),
            "a lodge footprint containing {terrain:?} should be rejected"
        );
    }

    for terrain in [TerrainKind::Dirt, TerrainKind::Sand, TerrainKind::Water] {
        let mut world = forestry_world();
        let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
        set_terrain(&mut world, CellCoord::new(3, 1), terrain);

        assert_eq!(
            place_tree_plot_blueprint(&mut world, lodge, CellCoord::new(3, 1)),
            Err(TreePlotPlacementError::InvalidTerrain),
            "a Tree Plot on {terrain:?} should be rejected"
        );
    }
}

#[test]
fn completed_lodge_and_tree_plot_gain_forestry_components() {
    let mut world = forestry_world();
    let lodge = spawn_blueprint_with_progress(
        &mut world,
        BuildingKind::ForesterLodge,
        CellCoord::new(0, 0),
        BuildingKind::ForesterLodge.definition().construction_cost(),
    );
    let plot = spawn_blueprint_with_progress(
        &mut world,
        BuildingKind::TreePlot,
        CellCoord::new(3, 1),
        BuildingKind::TreePlot.definition().construction_cost(),
    );
    world.entity_mut(plot).insert(TreePlotOwner::new(lodge));

    world
        .run_system_once(system_complete_building_construction)
        .expect("completion system should run");

    let inventory = world
        .get::<ForesterLodgeInventory>(lodge)
        .expect("completed lodge should gain an inventory");
    assert_eq!(inventory.wood(), 0);
    assert_eq!(inventory.max_size(), FORESTER_LODGE_INVENTORY_MAX_WOOD);
    assert_eq!(
        *world
            .get::<TreePlotGrowth>(plot)
            .expect("completed Tree Plot should gain growth state"),
        TreePlotGrowth::seedable()
    );
    assert_eq!(
        world
            .get::<TreePlotOwner>(plot)
            .expect("Tree Plot should retain its owner")
            .forester_lodge(),
        lodge
    );
}

#[test]
fn plots_can_link_to_lodge_blueprints_but_wait_for_both_buildings_to_complete() {
    let mut world = forestry_world();
    let lodge = spawn_blueprint_with_progress(
        &mut world,
        BuildingKind::ForesterLodge,
        CellCoord::new(0, 0),
        ResourceAmounts::zero(),
    );
    let plot = place_tree_plot_blueprint(&mut world, lodge, CellCoord::new(3, 1))
        .expect("Tree Plot should link to a Lodge blueprint");

    world.entity_mut(plot).insert(ConstructionProgress::new(
        BuildingKind::TreePlot.definition().construction_cost(),
    ));
    run_complete_building_construction(&mut world);
    assert!(world.get::<TreePlotGrowth>(plot).is_some());
    assert!(world.get::<ForesterLodgeInventory>(lodge).is_none());

    run_maintain_forestry_tasks(&mut world);
    assert!(seed_tasks(&mut world).is_empty());

    world.entity_mut(lodge).insert(ConstructionProgress::new(
        BuildingKind::ForesterLodge.definition().construction_cost(),
    ));
    run_complete_building_construction(&mut world);
    run_maintain_forestry_tasks(&mut world);

    assert_eq!(seed_tasks(&mut world), vec![plot]);
}

#[test]
fn tree_plot_placement_chains_cardinally_and_batches_in_row_major_order() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(5, 5));

    let result = place_tree_plot_blueprints(
        &mut world,
        lodge,
        [
            CellCoord::new(5, 3),
            CellCoord::new(5, 4),
            CellCoord::new(5, 3),
            CellCoord::new(4, 4),
            CellCoord::new(9, 9),
        ],
    );

    assert_eq!(
        result
            .placed
            .iter()
            .map(|placed| placed.coord)
            .collect::<Vec<_>>(),
        vec![
            CellCoord::new(5, 3),
            CellCoord::new(4, 4),
            CellCoord::new(5, 4),
        ]
    );
    assert_eq!(result.rejected.len(), 1);
    assert_eq!(result.rejected[0].coord, CellCoord::new(9, 9));
    assert_eq!(
        result.rejected[0].error,
        TreePlotPlacementError::NotConnected
    );
}

#[test]
fn tree_plot_limit_counts_blueprints_and_constructed_plots() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));

    for index in 0..MAX_TREE_PLOTS_PER_FORESTER_LODGE {
        let coord = CellCoord::new(index as i32 + 3, 0);
        if index % 2 == 0 {
            world.spawn((
                Building::new(BuildingKind::TreePlot, BuildingFootprint::new(coord, 1, 1)),
                TreePlotOwner::new(lodge),
                TreePlotGrowth::seedable(),
            ));
        } else {
            world.spawn((
                BuildingBlueprint {
                    kind: BuildingKind::TreePlot,
                    footprint: BuildingFootprint::new(coord, 1, 1),
                },
                ConstructionProgress::new(ResourceAmounts::zero()),
                TreePlotOwner::new(lodge),
            ));
        }
    }

    assert_eq!(
        forester_lodge_tree_plot_counts(&world, lodge),
        (
            MAX_TREE_PLOTS_PER_FORESTER_LODGE,
            MAX_TREE_PLOTS_PER_FORESTER_LODGE / 2
        )
    );
    assert_eq!(
        place_tree_plot_blueprint(&mut world, lodge, CellCoord::new(3, 1)),
        Err(TreePlotPlacementError::ForesterLodgeTreePlotLimitReached)
    );
}

#[test]
fn tree_plot_batch_limit_keeps_the_connector_instead_of_a_disconnected_plot() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(5, 5));

    for index in 0..MAX_TREE_PLOTS_PER_FORESTER_LODGE - 1 {
        world.spawn((
            Building::new(
                BuildingKind::TreePlot,
                BuildingFootprint::new(CellCoord::new(1_000 + index as i32, 1_000), 1, 1),
            ),
            TreePlotOwner::new(lodge),
            TreePlotGrowth::seedable(),
        ));
    }

    let distal = CellCoord::new(5, 3);
    let connector = CellCoord::new(5, 4);
    let result = place_tree_plot_blueprints(&mut world, lodge, [distal, connector]);

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
        TreePlotPlacementError::ForesterLodgeTreePlotLimitReached
    );
}

#[test]
fn simulation_tree_plot_placement_is_surface_scoped() {
    let mut simulation = GameSimulation::new(TEST_GENERATION_SEED);
    let first = simulation.create_surface(GridSize::new(8, 8));
    let second = simulation.create_surface(GridSize::new(8, 8));
    let lodge = simulation
        .place_building_blueprint(first, BuildingKind::ForesterLodge, CellCoord::new(1, 1))
        .expect("lodge should place");

    assert_eq!(
        simulation.place_tree_plot_blueprint(second, lodge, CellCoord::new(4, 2)),
        Err(TreePlotPlacementError::OwnerMissing)
    );
}

#[test]
fn forestry_task_maintenance_tracks_seedable_and_mature_plots() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let seedable = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::seedable(),
    );
    let mature = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 2),
        TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS),
    );

    run_maintain_forestry_tasks(&mut world);
    run_maintain_forestry_tasks(&mut world);

    assert_eq!(seed_tasks(&mut world), vec![seedable]);
    assert_eq!(cut_tasks(&mut world), vec![mature]);
}

#[test]
fn tree_plot_state_has_exact_growth_phase_boundaries() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS / 2 - 1),
    );

    assert_eq!(tree_plot_state(&world, plot), Some(TreePlotState::Sapling));
    run_advance_tree_growth(&mut world);
    assert_eq!(tree_plot_state(&world, plot), Some(TreePlotState::Young));

    world
        .entity_mut(plot)
        .insert(TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS - 1));
    assert_eq!(tree_plot_state(&world, plot), Some(TreePlotState::Young));
    run_advance_tree_growth(&mut world);
    assert_eq!(tree_plot_state(&world, plot), Some(TreePlotState::Mature));
}

#[test]
fn tree_growth_continues_after_its_lodge_is_removed() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS - 1),
    );
    world.despawn(lodge);

    run_advance_tree_growth(&mut world);

    assert_eq!(
        world.get::<TreePlotGrowth>(plot).unwrap().growth_ticks(),
        Some(TREE_PLOT_GROWTH_TICKS)
    );
    assert_eq!(tree_plot_state(&world, plot), Some(TreePlotState::Inactive));
}

#[test]
fn seeding_finishes_at_7200_ticks_and_awards_lumberjack_xp() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::with_seeding_progress(TREE_PLOT_SEEDING_TICKS - 1),
    );
    let npc = world
        .spawn((
            Npc,
            Forester,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiSeedTreePlot::new(plot),
        ))
        .id();

    run_seed_tree_plots(&mut world);

    let growth = *world.get::<TreePlotGrowth>(plot).unwrap();
    assert_eq!(growth.seeding_progress_ticks(), TREE_PLOT_SEEDING_TICKS);
    assert_eq!(growth.growth_ticks(), Some(0));
    assert!(world.get::<AiSeedTreePlot>(npc).is_none());
    assert_eq!(npc_skill(&world, npc, SkillKind::Lumberjack), 1);
}

#[test]
fn seeding_interruption_preserves_plot_progress_without_xp() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::with_seeding_progress(12),
    );
    let npc = world
        .spawn((
            Npc,
            Forester,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiSeedTreePlot::new(plot),
            AiSearchForFood,
        ))
        .id();

    run_seed_tree_plots(&mut world);

    assert_eq!(
        world
            .get::<TreePlotGrowth>(plot)
            .unwrap()
            .seeding_progress_ticks(),
        12
    );
    assert!(world.get::<AiSeedTreePlot>(npc).is_none());
    assert_eq!(npc_skill(&world, npc, SkillKind::Lumberjack), 0);
}

#[test]
fn cutting_takes_60_productive_ticks_then_adds_one_wood_and_resets_plot() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS),
    );
    let npc = world
        .spawn((
            Npc,
            Forester,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiCutTreePlot::new(plot),
        ))
        .id();

    for _ in 0..TREE_PLOT_CUTTING_TICKS - 1 {
        run_cut_tree_plots(&mut world);
    }

    assert_eq!(tree_plot_state(&world, plot), Some(TreePlotState::Mature));
    assert_eq!(
        world.get::<ForesterLodgeInventory>(lodge).unwrap().wood(),
        0
    );
    assert_eq!(
        world
            .get::<AiCutTreePlot>(npc)
            .expect("cut work should still be active")
            .progress_ticks(),
        TREE_PLOT_CUTTING_TICKS - 1
    );
    assert_eq!(npc_skill(&world, npc, SkillKind::Lumberjack), 0);

    run_cut_tree_plots(&mut world);

    assert_eq!(
        world.get::<ForesterLodgeInventory>(lodge).unwrap().wood(),
        1
    );
    assert_eq!(
        *world.get::<TreePlotGrowth>(plot).unwrap(),
        TreePlotGrowth::seedable()
    );
    assert!(world.get::<AiCutTreePlot>(npc).is_none());
    assert_eq!(npc_skill(&world, npc, SkillKind::Lumberjack), 1);
}

#[test]
fn cutting_interruption_discards_worker_progress_without_consuming_tree() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS),
    );
    let npc = world
        .spawn((
            Npc,
            Forester,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiCutTreePlot::new(plot),
        ))
        .id();

    for _ in 0..12 {
        run_cut_tree_plots(&mut world);
    }
    assert_eq!(
        world.get::<AiCutTreePlot>(npc).unwrap().progress_ticks(),
        12
    );

    world.entity_mut(npc).insert(AiSearchForFood);
    run_cut_tree_plots(&mut world);

    assert!(world.get::<AiCutTreePlot>(npc).is_none());
    assert_eq!(tree_plot_state(&world, plot), Some(TreePlotState::Mature));
    assert_eq!(
        world.get::<ForesterLodgeInventory>(lodge).unwrap().wood(),
        0
    );
    assert_eq!(npc_skill(&world, npc, SkillKind::Lumberjack), 0);
}

#[test]
fn full_lodge_inventory_suppresses_cutting_without_losing_the_tree() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    fill_lodge_inventory(&mut world, lodge);
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS),
    );
    let npc = world
        .spawn((
            Npc,
            Forester,
            NpcPosition::new(CellCoord::new(3, 1)),
            NpcSkills::default(),
            AiCutTreePlot::new(plot),
        ))
        .id();

    run_maintain_forestry_tasks(&mut world);
    assert!(cut_tasks(&mut world).is_empty());
    run_cut_tree_plots(&mut world);

    assert!(world.get::<AiCutTreePlot>(npc).is_none());
    assert_eq!(tree_plot_state(&world, plot), Some(TreePlotState::Mature));
    assert_eq!(
        world.get::<ForesterLodgeInventory>(lodge).unwrap().wood(),
        FORESTER_LODGE_INVENTORY_MAX_WOOD
    );
    assert_eq!(npc_skill(&world, npc, SkillKind::Lumberjack), 0);
}

#[test]
fn cutting_task_returns_when_lodge_capacity_becomes_available() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    fill_lodge_inventory(&mut world, lodge);
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS),
    );

    run_maintain_forestry_tasks(&mut world);
    assert!(cut_tasks(&mut world).is_empty());

    assert!(world
        .get_mut::<ForesterLodgeInventory>(lodge)
        .expect("lodge should have inventory")
        .consume_wood(1));
    run_maintain_forestry_tasks(&mut world);

    assert_eq!(cut_tasks(&mut world), vec![plot]);
}

#[test]
fn plot_assignment_requires_the_matching_role() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::seedable(),
    );
    run_maintain_forestry_tasks(&mut world);
    let forester = spawn_available_npc(&mut world, CellCoord::new(2, 1), (false, true));
    let farmer = spawn_available_npc(&mut world, CellCoord::new(2, 2), (true, false));
    let unassigned = spawn_available_npc(&mut world, CellCoord::new(1, 1), (false, false));

    run_assign_plot_work(&mut world);

    assert_eq!(
        world
            .get::<AiSeedTreePlot>(forester)
            .expect("Forester should take Tree Plot work")
            .tree_plot(),
        plot
    );
    assert!(world.get::<AiSeedTreePlot>(farmer).is_none());
    assert!(world.get::<AiSeedTreePlot>(unassigned).is_none());
}

#[test]
fn combined_assignment_chooses_nearest_farm_or_forestry_work() {
    let mut world = forestry_world();
    let farm = spawn_farm(&mut world, CellCoord::new(10, 10));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(9, 10),
        FieldCrop::seedable(),
    );
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::seedable(),
    );
    run_maintain_farming_tasks(&mut world);
    run_maintain_forestry_tasks(&mut world);
    let worker = spawn_available_npc(&mut world, CellCoord::new(2, 1), (true, true));

    run_assign_plot_work(&mut world);

    assert_eq!(
        world
            .get::<AiSeedTreePlot>(worker)
            .expect("worker should prefer nearby forestry work")
            .tree_plot(),
        plot
    );
    assert!(world
        .get::<game_engine::farming::AiSeedField>(worker)
        .is_none());
    assert!(world.get::<FieldCrop>(field).is_some());
}

#[test]
fn combined_assignment_reserves_a_plot_immediately_for_only_one_worker() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::seedable(),
    );
    run_maintain_forestry_tasks(&mut world);
    let first = spawn_available_npc(&mut world, CellCoord::new(2, 1), (false, true));
    let second = spawn_available_npc(&mut world, CellCoord::new(2, 2), (false, true));

    run_assign_plot_work(&mut world);

    let claims = [first, second]
        .into_iter()
        .filter(|npc| {
            world
                .get::<AiSeedTreePlot>(*npc)
                .is_some_and(|work| work.tree_plot() == plot)
        })
        .count();
    assert_eq!(claims, 1);
}

#[test]
fn combined_assignment_reserves_a_field_immediately_for_only_one_worker() {
    let mut world = forestry_world();
    let farm = spawn_farm(&mut world, CellCoord::new(0, 0));
    let field = spawn_field(
        &mut world,
        farm,
        CellCoord::new(3, 1),
        FieldCrop::seedable(),
    );
    run_maintain_farming_tasks(&mut world);
    let first = spawn_available_npc(&mut world, CellCoord::new(2, 1), (true, false));
    let second = spawn_available_npc(&mut world, CellCoord::new(2, 2), (true, false));

    run_assign_plot_work(&mut world);

    let claims = [first, second]
        .into_iter()
        .filter(|npc| {
            world
                .get::<AiSeedField>(*npc)
                .is_some_and(|work| work.field() == field)
        })
        .count();
    assert_eq!(claims, 1);
}

#[test]
fn combined_assignment_claims_distinct_nearest_plots_deterministically() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    let first_plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::seedable(),
    );
    let second_plot = spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 2),
        TreePlotGrowth::seedable(),
    );
    run_maintain_forestry_tasks(&mut world);
    let first_worker = spawn_available_npc(&mut world, CellCoord::new(2, 1), (false, true));
    let second_worker = spawn_available_npc(&mut world, CellCoord::new(2, 2), (false, true));

    run_assign_plot_work(&mut world);

    assert_eq!(
        world
            .get::<AiSeedTreePlot>(first_worker)
            .expect("first worker should receive work")
            .tree_plot(),
        first_plot
    );
    assert_eq!(
        world
            .get::<AiSeedTreePlot>(second_worker)
            .expect("second worker should receive work")
            .tree_plot(),
        second_plot
    );
}

#[test]
fn food_and_construction_work_preempt_forestry_assignment() {
    let mut world = forestry_world();
    let lodge = spawn_lodge(&mut world, CellCoord::new(0, 0));
    spawn_tree_plot(
        &mut world,
        lodge,
        CellCoord::new(3, 1),
        TreePlotGrowth::seedable(),
    );
    run_maintain_forestry_tasks(&mut world);
    let food_worker = spawn_available_npc(&mut world, CellCoord::new(2, 1), (false, true));
    world.entity_mut(food_worker).insert(AiSearchForFood);
    let construction_worker = spawn_available_npc(&mut world, CellCoord::new(2, 2), (false, true));
    let blueprint = world.spawn_empty().id();
    world
        .entity_mut(construction_worker)
        .insert(game_engine::components::AiConstructBuilding::new(blueprint));

    run_assign_plot_work(&mut world);

    assert!(world.get::<AiSeedTreePlot>(food_worker).is_none());
    assert!(world.get::<AiSeedTreePlot>(construction_worker).is_none());
}

fn forestry_world() -> World {
    let size = GridSize::new(32, 32);
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

fn spawn_lodge(world: &mut World, origin: CellCoord) -> Entity {
    world
        .spawn((
            Building::new(
                BuildingKind::ForesterLodge,
                BuildingFootprint::new(origin, 3, 3),
            ),
            ForesterLodgeInventory::empty(),
        ))
        .id()
}

fn spawn_tree_plot(
    world: &mut World,
    lodge: Entity,
    coord: CellCoord,
    growth: TreePlotGrowth,
) -> Entity {
    world
        .spawn((
            Building::new(BuildingKind::TreePlot, BuildingFootprint::new(coord, 1, 1)),
            TreePlotOwner::new(lodge),
            growth,
        ))
        .id()
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

fn spawn_available_npc(
    world: &mut World,
    coord: CellCoord,
    (farmer, forester): (bool, bool),
) -> Entity {
    let mut entity = world.spawn((Npc, NpcPosition::new(coord), NpcInventory::empty()));
    if farmer {
        entity.insert(Farmer);
    }
    if forester {
        entity.insert(Forester);
    }
    entity.id()
}

fn set_terrain(world: &mut World, coord: CellCoord, terrain: TerrainKind) {
    let tile = world
        .resource::<TileIndex>()
        .get(coord)
        .expect("test tile should exist");
    world
        .get_mut::<Terrain>(tile)
        .expect("test tile should have terrain")
        .kind = terrain;
}

fn fill_lodge_inventory(world: &mut World, lodge: Entity) {
    let mut inventory = world
        .get_mut::<ForesterLodgeInventory>(lodge)
        .expect("lodge should have inventory");
    for _ in 0..FORESTER_LODGE_INVENTORY_MAX_WOOD {
        assert!(inventory.add_wood(1));
    }
}

fn run_maintain_forestry_tasks(world: &mut World) {
    world
        .run_system_once(maintain_forestry_tasks)
        .expect("forestry maintenance should run");
}

fn run_complete_building_construction(world: &mut World) {
    world
        .run_system_once(system_complete_building_construction)
        .expect("building completion should run");
}

fn run_maintain_farming_tasks(world: &mut World) {
    world
        .run_system_once(maintain_farming_tasks)
        .expect("farming maintenance should run");
}

fn run_assign_plot_work(world: &mut World) {
    world
        .run_system_once(system_assign_plot_work)
        .expect("combined plot assignment should run");
}

fn run_seed_tree_plots(world: &mut World) {
    world
        .run_system_once(system_seed_tree_plots)
        .expect("Tree Plot seeding should run");
}

fn run_advance_tree_growth(world: &mut World) {
    world
        .run_system_once(system_advance_tree_growth)
        .expect("tree growth should run");
}

fn run_cut_tree_plots(world: &mut World) {
    world
        .run_system_once(system_cut_tree_plots)
        .expect("Tree Plot cutting should run");
}

fn seed_tasks(world: &mut World) -> Vec<Entity> {
    let mut tasks = world
        .try_query::<&SeedTreePlot>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|task| task.tree_plot())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    tasks.sort_by_key(|entity| entity.to_bits());
    tasks
}

fn cut_tasks(world: &mut World) -> Vec<Entity> {
    let mut tasks = world
        .try_query::<&CutTreePlot>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|task| task.tree_plot())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    tasks.sort_by_key(|entity| entity.to_bits());
    tasks
}

fn npc_skill(world: &World, npc: Entity, kind: SkillKind) -> u32 {
    world
        .get::<NpcSkills>(npc)
        .expect("NPC should have skills")
        .value(kind)
}
