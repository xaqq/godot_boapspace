use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::buildings::{Building, BuildingBlueprint, BuildingFootprint, BuildingKind};
use game_engine::components::{
    AiGatherResource, CarriedResource, MovementFacing, Npc, NpcAppearance, NpcPosition,
    ResourceNode, TerrainKind, Tile, TilePosition, Velocity, Wheelbarrow,
};
use game_engine::farming::{
    field_crop_state, AiHarvestField, AiSeedField, FieldCrop, FieldCropState,
};
use game_engine::forestry::{
    tree_plot_state, AiCutTreePlot, AiSeedTreePlot, TreePlotGrowth, TreePlotState,
};
use game_engine::grid::{CellCoord, GridSize};
use game_engine::refining::{npc_refining_activity, RefiningActivity};
use game_engine::resources::ResourceKind;
use game_engine::roads::{Road, RoadBlueprint, RoadTier};
use game_engine::simulation::SurfaceId;
use std::collections::HashSet;

const ROAD_NORTH: u8 = 1;
const ROAD_EAST: u8 = 2;
const ROAD_SOUTH: u8 = 4;
const ROAD_WEST: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainRenderCell {
    pub(crate) coord: CellCoord,
    pub(crate) kind: TerrainKind,
    pub(crate) variant: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SurfaceRenderSnapshot {
    pub(crate) surface_id: SurfaceId,
    pub(crate) size: GridSize,
    pub(crate) terrain_cells: Vec<TerrainRenderCell>,
}

impl SurfaceRenderSnapshot {
    pub(crate) fn new(
        surface_id: SurfaceId,
        size: GridSize,
        mut terrain_cells: Vec<TerrainRenderCell>,
    ) -> Self {
        terrain_cells.sort_by_key(|cell| (cell.coord.y(), cell.coord.x()));
        Self {
            surface_id,
            size,
            terrain_cells,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoadRenderCell {
    pub(crate) entity: Entity,
    pub(crate) coord: CellCoord,
    pub(crate) tier: RoadTier,
    pub(crate) connectivity_mask: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuildingRenderState {
    Blueprint,
    Constructed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildingRenderInfo {
    pub(crate) entity: Entity,
    pub(crate) kind: BuildingKind,
    pub(crate) footprint: BuildingFootprint,
    pub(crate) state: BuildingRenderState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResourceRenderInfo {
    pub(crate) entity: Entity,
    pub(crate) coord: CellCoord,
    pub(crate) kind: ResourceKind,
    pub(crate) quantity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CropRenderInfo {
    pub(crate) entity: Entity,
    pub(crate) coord: CellCoord,
    pub(crate) state: FieldCropState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TreePlotRenderInfo {
    pub(crate) entity: Entity,
    pub(crate) coord: CellCoord,
    pub(crate) state: TreePlotState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NpcActivity {
    Idle,
    Walk,
    Gather,
    Saw,
    Stonecut,
    Cook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NpcRenderInfo {
    pub(crate) entity: Entity,
    pub(crate) appearance: NpcAppearance,
    pub(crate) position: NpcPosition,
    pub(crate) velocity: Velocity,
    pub(crate) facing: MovementFacing,
    pub(crate) activity: NpcActivity,
    pub(crate) carried_kind: Option<ResourceKind>,
    pub(crate) has_wheelbarrow: bool,
    pub(crate) wheelbarrow_kind: Option<ResourceKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DynamicRenderSnapshot {
    pub(crate) completed_roads: Vec<RoadRenderCell>,
    pub(crate) planned_roads: Vec<RoadRenderCell>,
    pub(crate) buildings: Vec<BuildingRenderInfo>,
    pub(crate) resources: Vec<ResourceRenderInfo>,
    pub(crate) crops: Vec<CropRenderInfo>,
    pub(crate) tree_plots: Vec<TreePlotRenderInfo>,
    pub(crate) npcs: Vec<NpcRenderInfo>,
}

impl DynamicRenderSnapshot {
    pub(crate) fn from_world(world: &World) -> Self {
        let (completed_roads, planned_roads) = query_roads(world);
        Self {
            completed_roads,
            planned_roads,
            buildings: query_buildings(world),
            resources: query_resources(world),
            crops: query_crops(world),
            tree_plots: query_tree_plots(world),
            npcs: query_npcs(world),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedCellOverlay {
    pub(crate) entity: Entity,
    pub(crate) coord: CellCoord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedNpcOverlay {
    pub(crate) entity: Entity,
    pub(crate) position: NpcPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedBuildingOverlay {
    pub(crate) entity: Entity,
    pub(crate) footprint: BuildingFootprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NpcRouteOverlay {
    Route {
        position: NpcPosition,
        waypoints: Vec<CellCoord>,
        destination: CellCoord,
    },
    Blocked {
        position: NpcPosition,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacementValidity {
    Valid,
    Invalid,
}

impl From<bool> for PlacementValidity {
    fn from(valid: bool) -> Self {
        if valid {
            Self::Valid
        } else {
            Self::Invalid
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacementCellOverlay {
    pub(crate) coord: CellCoord,
    pub(crate) validity: PlacementValidity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildingPreviewOverlay {
    pub(crate) kind: BuildingKind,
    pub(crate) footprint: BuildingFootprint,
    pub(crate) validity: PlacementValidity,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WorldOverlaySnapshot {
    pub(crate) selected_cell: Option<SelectedCellOverlay>,
    pub(crate) selected_npc: Option<SelectedNpcOverlay>,
    pub(crate) selected_building: Option<SelectedBuildingOverlay>,
    pub(crate) selected_npc_route: Option<NpcRouteOverlay>,
    pub(crate) building_preview: Option<BuildingPreviewOverlay>,
    pub(crate) plot_cells: Vec<PlacementCellOverlay>,
    pub(crate) road_cells: Vec<PlacementCellOverlay>,
}

fn query_roads(world: &World) -> (Vec<RoadRenderCell>, Vec<RoadRenderCell>) {
    let mut completed = world
        .try_query::<(Entity, &Road)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, road)| RoadRenderCell {
                    entity,
                    coord: road.coord,
                    tier: road.tier,
                    connectivity_mask: 0,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut planned = world
        .try_query::<(Entity, &RoadBlueprint)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, road)| RoadRenderCell {
                    entity,
                    coord: road.coord,
                    tier: road.target_tier,
                    connectivity_mask: 0,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let completed_cells = completed
        .iter()
        .map(|road| road.coord)
        .collect::<HashSet<_>>();
    let planned_cells = planned
        .iter()
        .map(|road| road.coord)
        .collect::<HashSet<_>>();
    let complete_and_planned_cells = completed_cells
        .union(&planned_cells)
        .copied()
        .collect::<HashSet<_>>();

    for road in &mut completed {
        road.connectivity_mask = road_connectivity_mask(road.coord, &completed_cells);
    }
    for road in &mut planned {
        road.connectivity_mask = road_connectivity_mask(road.coord, &complete_and_planned_cells);
    }

    completed.sort_by_key(road_sort_key);
    planned.sort_by_key(road_sort_key);
    (completed, planned)
}

fn road_sort_key(road: &RoadRenderCell) -> (i32, i32, u64) {
    (road.coord.y(), road.coord.x(), road.entity.to_bits())
}

pub(crate) fn road_connectivity_mask(coord: CellCoord, cells: &HashSet<CellCoord>) -> u8 {
    let mut mask = 0;
    for (bit, x_offset, y_offset) in [
        (ROAD_NORTH, 0, -1),
        (ROAD_EAST, 1, 0),
        (ROAD_SOUTH, 0, 1),
        (ROAD_WEST, -1, 0),
    ] {
        let Some(x) = coord.x().checked_add(x_offset) else {
            continue;
        };
        let Some(y) = coord.y().checked_add(y_offset) else {
            continue;
        };
        if cells.contains(&CellCoord::new(x, y)) {
            mask |= bit;
        }
    }
    mask
}

fn query_buildings(world: &World) -> Vec<BuildingRenderInfo> {
    let mut buildings = world
        .try_query::<(Entity, &BuildingBlueprint)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, blueprint)| BuildingRenderInfo {
                    entity,
                    kind: blueprint.kind,
                    footprint: blueprint.footprint,
                    state: BuildingRenderState::Blueprint,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if let Some(mut query) = world.try_query::<(Entity, &Building)>() {
        buildings.extend(
            query
                .iter(world)
                .map(|(entity, building)| BuildingRenderInfo {
                    entity,
                    kind: building.kind,
                    footprint: building.footprint,
                    state: BuildingRenderState::Constructed,
                }),
        );
    }

    buildings.sort_by_key(|building| building.entity.to_bits());
    buildings
}

fn query_resources(world: &World) -> Vec<ResourceRenderInfo> {
    let mut resources = world
        .try_query::<(Entity, &TilePosition, &ResourceNode, &Tile)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, position, resource, _)| ResourceRenderInfo {
                    entity,
                    coord: position.coord,
                    kind: resource.kind,
                    quantity: resource.quantity,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    resources.sort_by_key(|resource| {
        (
            resource.coord.y(),
            resource.coord.x(),
            resource.entity.to_bits(),
        )
    });
    resources
}

fn query_crops(world: &World) -> Vec<CropRenderInfo> {
    let mut crops = world
        .try_query::<(Entity, &Building, &FieldCrop)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter_map(|(entity, building, _)| {
                    (building.kind == BuildingKind::Field).then_some(())?;
                    Some(CropRenderInfo {
                        entity,
                        coord: building.footprint.origin(),
                        state: field_crop_state(world, entity)?,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    crops.sort_by_key(|crop| (crop.coord.y(), crop.coord.x(), crop.entity.to_bits()));
    crops
}

fn query_tree_plots(world: &World) -> Vec<TreePlotRenderInfo> {
    let mut tree_plots = world
        .try_query::<(Entity, &Building, &TreePlotGrowth)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter_map(|(entity, building, _)| {
                    (building.kind == BuildingKind::TreePlot).then_some(())?;
                    Some(TreePlotRenderInfo {
                        entity,
                        coord: building.footprint.origin(),
                        state: tree_plot_state(world, entity)?,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    tree_plots.sort_by_key(|tree_plot| {
        (
            tree_plot.coord.y(),
            tree_plot.coord.x(),
            tree_plot.entity.to_bits(),
        )
    });
    tree_plots
}

fn query_npcs(world: &World) -> Vec<NpcRenderInfo> {
    let mut npcs = world
        .try_query::<(Entity, &NpcPosition, Option<&NpcAppearance>, &Npc)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, position, appearance, _)| {
                    let velocity = world.get::<Velocity>(entity).copied().unwrap_or_default();
                    let is_gathering = world.get::<AiGatherResource>(entity).is_some()
                        || world.get::<AiSeedField>(entity).is_some()
                        || world.get::<AiHarvestField>(entity).is_some()
                        || world.get::<AiSeedTreePlot>(entity).is_some()
                        || world.get::<AiCutTreePlot>(entity).is_some();
                    let wheelbarrow = world.get::<Wheelbarrow>(entity).copied();
                    NpcRenderInfo {
                        entity,
                        appearance: appearance.copied().unwrap_or_default(),
                        position: *position,
                        velocity,
                        facing: world
                            .get::<MovementFacing>(entity)
                            .copied()
                            .unwrap_or_default(),
                        activity: resolve_npc_activity(
                            npc_refining_activity(world, entity),
                            is_gathering,
                            velocity,
                        ),
                        carried_kind: world
                            .get::<CarriedResource>(entity)
                            .and_then(|cargo| cargo.stack())
                            .map(|stack| stack.kind()),
                        has_wheelbarrow: wheelbarrow.is_some(),
                        wheelbarrow_kind: wheelbarrow
                            .and_then(|cargo| cargo.stack())
                            .map(|stack| stack.kind()),
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    npcs.sort_by_key(|npc| npc.entity.to_bits());
    npcs
}

pub(crate) fn resolve_npc_activity(
    refining: Option<RefiningActivity>,
    is_gathering: bool,
    velocity: Velocity,
) -> NpcActivity {
    if let Some(refining) = refining {
        return match refining {
            RefiningActivity::Saw => NpcActivity::Saw,
            RefiningActivity::Stonecut => NpcActivity::Stonecut,
            RefiningActivity::Cook => NpcActivity::Cook,
        };
    }
    if is_gathering {
        return NpcActivity::Gather;
    }
    if !velocity.is_zero() {
        return NpcActivity::Walk;
    }
    NpcActivity::Idle
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::farming::{FarmInventory, FieldOwner, FIELD_GROWTH_TICKS};
    use game_engine::forestry::{ForesterLodgeInventory, TreePlotOwner, TREE_PLOT_GROWTH_TICKS};
    use game_engine::simulation::GameSimulation;

    #[test]
    fn surface_snapshot_sorts_supplied_terrain_row_major() {
        let simulation = GameSimulation::new(7);
        let surface_id = simulation.default_surface_id();
        let snapshot = SurfaceRenderSnapshot::new(
            surface_id,
            GridSize::new(3, 2),
            vec![
                TerrainRenderCell {
                    coord: CellCoord::new(2, 1),
                    kind: TerrainKind::Sand,
                    variant: 3,
                },
                TerrainRenderCell {
                    coord: CellCoord::new(1, 0),
                    kind: TerrainKind::Dirt,
                    variant: 2,
                },
                TerrainRenderCell {
                    coord: CellCoord::new(0, 0),
                    kind: TerrainKind::Grass,
                    variant: 1,
                },
            ],
        );

        assert_eq!(snapshot.surface_id, surface_id);
        assert_eq!(snapshot.size, GridSize::new(3, 2));
        assert_eq!(
            snapshot
                .terrain_cells
                .iter()
                .map(|cell| cell.coord)
                .collect::<Vec<_>>(),
            vec![
                CellCoord::new(0, 0),
                CellCoord::new(1, 0),
                CellCoord::new(2, 1),
            ]
        );
    }

    #[test]
    fn dynamic_snapshot_is_complete_and_deterministically_sorted() {
        let mut world = World::new();
        world.spawn(Road {
            coord: CellCoord::new(4, 3),
            tier: RoadTier::Flagstone,
        });
        world.spawn(Road {
            coord: CellCoord::new(1, 0),
            tier: RoadTier::DirtPath,
        });
        world.spawn(RoadBlueprint {
            coord: CellCoord::new(3, 2),
            target_tier: RoadTier::Cobblestone,
        });
        world.spawn(RoadBlueprint {
            coord: CellCoord::new(0, 1),
            target_tier: RoadTier::DirtPath,
        });

        let blueprint = world
            .spawn(BuildingBlueprint {
                kind: BuildingKind::Warehouse,
                footprint: BuildingFootprint::new(CellCoord::new(8, 8), 4, 4),
            })
            .id();
        let constructed = world
            .spawn(Building::new(
                BuildingKind::Depot,
                BuildingFootprint::new(CellCoord::new(2, 2), 2, 2),
            ))
            .id();

        let late_resource = world
            .spawn((
                Tile,
                TilePosition {
                    coord: CellCoord::new(5, 5),
                },
                ResourceNode {
                    kind: ResourceKind::Stone,
                    quantity: 70,
                },
            ))
            .id();
        let early_resource = world
            .spawn((
                Tile,
                TilePosition {
                    coord: CellCoord::new(2, 1),
                },
                ResourceNode {
                    kind: ResourceKind::Wood,
                    quantity: 90,
                },
            ))
            .id();

        let gather_target = world.spawn_empty().id();
        let first_npc = world
            .spawn((
                Npc,
                NpcPosition::new(CellCoord::new(9, 9)),
                NpcAppearance::Scout,
                Velocity::new(1, 0),
                MovementFacing::East,
                AiGatherResource::new(gather_target),
                CarriedResource::of(ResourceKind::Gold, 2),
                Wheelbarrow::of(ResourceKind::Stone, 4),
            ))
            .id();
        let second_npc = world
            .spawn((Npc, NpcPosition::new(CellCoord::new(1, 1))))
            .id();

        let snapshot = DynamicRenderSnapshot::from_world(&world);

        assert_eq!(
            snapshot
                .completed_roads
                .iter()
                .map(|road| road.coord)
                .collect::<Vec<_>>(),
            vec![CellCoord::new(1, 0), CellCoord::new(4, 3)]
        );
        assert_eq!(
            snapshot
                .planned_roads
                .iter()
                .map(|road| road.coord)
                .collect::<Vec<_>>(),
            vec![CellCoord::new(0, 1), CellCoord::new(3, 2)]
        );
        assert_eq!(snapshot.buildings.len(), 2);
        let mut expected_building_order = vec![blueprint, constructed];
        expected_building_order.sort_by_key(|entity| entity.to_bits());
        assert_eq!(
            snapshot
                .buildings
                .iter()
                .map(|building| building.entity)
                .collect::<Vec<_>>(),
            expected_building_order
        );
        assert_eq!(
            snapshot
                .buildings
                .iter()
                .find(|building| building.entity == blueprint)
                .expect("blueprint should render")
                .state,
            BuildingRenderState::Blueprint
        );
        assert_eq!(
            snapshot
                .buildings
                .iter()
                .find(|building| building.entity == constructed)
                .expect("building should render")
                .state,
            BuildingRenderState::Constructed
        );
        assert_eq!(
            snapshot
                .resources
                .iter()
                .map(|resource| (resource.entity, resource.coord, resource.quantity))
                .collect::<Vec<_>>(),
            vec![
                (early_resource, CellCoord::new(2, 1), 90),
                (late_resource, CellCoord::new(5, 5), 70),
            ]
        );
        let mut expected_npc_order = vec![first_npc, second_npc];
        expected_npc_order.sort_by_key(|entity| entity.to_bits());
        assert_eq!(
            snapshot
                .npcs
                .iter()
                .map(|npc| npc.entity)
                .collect::<Vec<_>>(),
            expected_npc_order
        );
        let npc = snapshot
            .npcs
            .iter()
            .find(|npc| npc.entity == first_npc)
            .copied()
            .expect("configured NPC should render");
        assert_eq!(npc.appearance, NpcAppearance::Scout);
        assert_eq!(npc.activity, NpcActivity::Gather);
        assert_eq!(npc.carried_kind, Some(ResourceKind::Gold));
        assert!(npc.has_wheelbarrow);
        assert_eq!(npc.wheelbarrow_kind, Some(ResourceKind::Stone));
    }

    #[test]
    fn npc_activity_uses_documented_precedence() {
        let walking = Velocity::new(1, 0);
        for (refining, expected) in [
            (RefiningActivity::Saw, NpcActivity::Saw),
            (RefiningActivity::Stonecut, NpcActivity::Stonecut),
            (RefiningActivity::Cook, NpcActivity::Cook),
        ] {
            assert_eq!(
                resolve_npc_activity(Some(refining), true, walking),
                expected
            );
        }
        assert_eq!(
            resolve_npc_activity(None, true, walking),
            NpcActivity::Gather
        );
        assert_eq!(
            resolve_npc_activity(None, false, walking),
            NpcActivity::Walk
        );
        assert_eq!(
            resolve_npc_activity(None, false, Velocity::ZERO),
            NpcActivity::Idle
        );
    }

    #[test]
    fn road_masks_use_completed_network_for_roads_and_union_for_plans() {
        let mut world = World::new();
        let center = CellCoord::new(2, 2);
        for coord in [
            center,
            CellCoord::new(2, 1),
            CellCoord::new(3, 2),
            CellCoord::new(2, 3),
            CellCoord::new(1, 2),
        ] {
            world.spawn(Road {
                coord,
                tier: RoadTier::DirtPath,
            });
        }
        world.spawn(RoadBlueprint {
            coord: CellCoord::new(4, 2),
            target_tier: RoadTier::Cobblestone,
        });
        world.spawn(RoadBlueprint {
            coord: CellCoord::new(5, 2),
            target_tier: RoadTier::Cobblestone,
        });

        let snapshot = DynamicRenderSnapshot::from_world(&world);
        let completed_center = snapshot
            .completed_roads
            .iter()
            .find(|road| road.coord == center)
            .expect("center road should render");
        assert_eq!(completed_center.connectivity_mask, 15);
        let completed_east = snapshot
            .completed_roads
            .iter()
            .find(|road| road.coord == CellCoord::new(3, 2))
            .expect("east road should render");
        assert_eq!(completed_east.connectivity_mask & ROAD_EAST, 0);

        let first_plan = snapshot
            .planned_roads
            .iter()
            .find(|road| road.coord == CellCoord::new(4, 2))
            .expect("planned road should render");
        assert_eq!(first_plan.connectivity_mask, ROAD_EAST | ROAD_WEST);
    }

    #[test]
    fn crop_and_tree_stages_follow_game_engine_mapping() {
        let mut world = World::new();
        let farm = world
            .spawn((
                Building::new(
                    BuildingKind::Farm,
                    BuildingFootprint::new(CellCoord::new(0, 0), 3, 3),
                ),
                FarmInventory::empty(),
            ))
            .id();
        let seedable_field = spawn_field(
            &mut world,
            farm,
            CellCoord::new(4, 0),
            FieldCrop::seedable(),
        );
        let seeding_field = spawn_field(
            &mut world,
            farm,
            CellCoord::new(5, 0),
            FieldCrop::seedable(),
        );
        world.spawn(AiSeedField::new(seeding_field));
        spawn_field(
            &mut world,
            farm,
            CellCoord::new(6, 0),
            FieldCrop::growing(1),
        );
        spawn_field(
            &mut world,
            farm,
            CellCoord::new(7, 0),
            FieldCrop::growing(FIELD_GROWTH_TICKS / 2),
        );
        spawn_field(
            &mut world,
            farm,
            CellCoord::new(8, 0),
            FieldCrop::growing(FIELD_GROWTH_TICKS),
        );

        let lodge = world
            .spawn((
                Building::new(
                    BuildingKind::ForesterLodge,
                    BuildingFootprint::new(CellCoord::new(0, 4), 3, 3),
                ),
                ForesterLodgeInventory::empty(),
            ))
            .id();
        spawn_tree_plot(
            &mut world,
            lodge,
            CellCoord::new(4, 4),
            TreePlotGrowth::seedable(),
        );
        let seeding_tree = spawn_tree_plot(
            &mut world,
            lodge,
            CellCoord::new(5, 4),
            TreePlotGrowth::seedable(),
        );
        world.spawn(AiSeedTreePlot::new(seeding_tree));
        spawn_tree_plot(
            &mut world,
            lodge,
            CellCoord::new(6, 4),
            TreePlotGrowth::growing(1),
        );
        spawn_tree_plot(
            &mut world,
            lodge,
            CellCoord::new(7, 4),
            TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS / 2),
        );
        spawn_tree_plot(
            &mut world,
            lodge,
            CellCoord::new(8, 4),
            TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS),
        );

        let snapshot = DynamicRenderSnapshot::from_world(&world);
        assert_eq!(
            snapshot
                .crops
                .iter()
                .map(|crop| crop.state)
                .collect::<Vec<_>>(),
            vec![
                FieldCropState::Seedable,
                FieldCropState::Seeding,
                FieldCropState::GrowingStep1,
                FieldCropState::GrowingStep2,
                FieldCropState::Grown,
            ]
        );
        assert_eq!(snapshot.crops[0].entity, seedable_field);
        assert_eq!(
            snapshot
                .tree_plots
                .iter()
                .map(|tree| tree.state)
                .collect::<Vec<_>>(),
            vec![
                TreePlotState::Seedable,
                TreePlotState::Seeding,
                TreePlotState::Sapling,
                TreePlotState::Young,
                TreePlotState::Mature,
            ]
        );
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
}
