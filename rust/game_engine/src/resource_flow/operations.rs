//! Cross-domain ECS dispatch for resource endpoint operations.
//!
//! This module intentionally depends on the concrete components owned by the
//! resource-producing and resource-consuming domains. It is an ECS integration
//! layer, not a dependency-free resource-flow model layer.

use bevy_ecs::prelude::{Entity, World};

use crate::buildings::{Building, BuildingActivity, StorageInventory};
use crate::components::{CarriedResource, NpcPosition, ResourceNode, TilePosition};
use crate::farming::FarmInventory;
use crate::forestry::ForesterLodgeInventory;
use crate::grid::CellCoord;
use crate::navigation::NavigationSnapshot;
use crate::refining::{recipes_for_building, RefineryInventory};
use crate::resources::ResourceKind;

use super::{SinkEndpoint, StockEndpoint};

pub(crate) fn stock_sources(
    world: &mut World,
    kind: ResourceKind,
    excluded_entity: Entity,
    worker: Entity,
) -> Vec<StockEndpoint> {
    let mut sources = Vec::new();
    if world
        .get::<CarriedResource>(worker)
        .is_some_and(|inventory| inventory.contents().get(kind) > 0)
    {
        sources.push(StockEndpoint::CarriedResource(worker));
    }
    if let Some(mut query) = world.try_query::<(Entity, &ResourceNode)>() {
        sources.extend(query.iter(world).filter_map(|(entity, node)| {
            (entity != excluded_entity && node.kind == kind && node.quantity > 0)
                .then_some(StockEndpoint::NaturalNode(entity))
        }));
    }
    if let Some(mut query) = world.try_query::<(Entity, &StorageInventory)>() {
        sources.extend(query.iter(world).filter_map(|(entity, inventory)| {
            (entity != excluded_entity
                && building_is_active(world, entity)
                && inventory.contents().get(kind) > 0)
                .then_some(StockEndpoint::Warehouse(entity))
        }));
    }
    if let Some(mut query) = world.try_query::<(Entity, &FarmInventory)>() {
        sources.extend(query.iter(world).filter_map(|(entity, inventory)| {
            (entity != excluded_entity && inventory.contents().get(kind) > 0)
                .then_some(StockEndpoint::Farm(entity))
        }));
    }
    if let Some(mut query) = world.try_query::<(Entity, &ForesterLodgeInventory)>() {
        sources.extend(query.iter(world).filter_map(|(entity, inventory)| {
            (entity != excluded_entity && inventory.contents().get(kind) > 0)
                .then_some(StockEndpoint::ForesterLodge(entity))
        }));
    }
    if let Some(mut query) = world.try_query::<(Entity, &RefineryInventory)>() {
        for (entity, inventory) in query
            .iter(world)
            .filter(|(entity, _)| *entity != excluded_entity && building_is_active(world, *entity))
        {
            if inventory.input_contents().get(kind) > 0 {
                sources.push(StockEndpoint::RefineryInput(entity));
            }
            if inventory.output_contents().get(kind) > 0 {
                sources.push(StockEndpoint::RefineryOutput(entity));
            }
        }
    }
    sources.sort_unstable_by_key(|source| (source.entity().to_bits(), endpoint_order(*source)));
    sources
}

pub(crate) fn source_interaction_cells(
    world: &World,
    snapshot: &NavigationSnapshot,
    source: StockEndpoint,
    worker: Entity,
) -> Vec<CellCoord> {
    if source == StockEndpoint::CarriedResource(worker) {
        return world
            .get::<NpcPosition>(worker)
            .map(|position| vec![position.coord])
            .unwrap_or_default();
    }
    match source {
        StockEndpoint::NaturalNode(entity) => world
            .get::<TilePosition>(entity)
            .map(|position| snapshot.point_interaction_cells(position.coord))
            .unwrap_or_default(),
        _ => world
            .get::<Building>(source.entity())
            .map(|building| snapshot.exterior_interaction_cells(building.footprint))
            .unwrap_or_default(),
    }
}

pub(crate) fn source_stock(world: &World, source: StockEndpoint, kind: ResourceKind) -> u32 {
    match source {
        StockEndpoint::NaturalNode(entity) => world
            .get::<ResourceNode>(entity)
            .filter(|node| node.kind == kind)
            .map_or(0, |node| node.quantity),
        StockEndpoint::CarriedResource(entity) => world
            .get::<CarriedResource>(entity)
            .map_or(0, |inventory| inventory.contents().get(kind)),
        StockEndpoint::Warehouse(entity) => world
            .get::<StorageInventory>(entity)
            .map_or(0, |inventory| inventory.contents().get(kind)),
        StockEndpoint::Farm(entity) => world
            .get::<FarmInventory>(entity)
            .map_or(0, |inventory| inventory.contents().get(kind)),
        StockEndpoint::ForesterLodge(entity) => world
            .get::<ForesterLodgeInventory>(entity)
            .map_or(0, |inventory| inventory.contents().get(kind)),
        StockEndpoint::RefineryInput(entity) => world
            .get::<RefineryInventory>(entity)
            .map_or(0, |inventory| inventory.input_contents().get(kind)),
        StockEndpoint::RefineryOutput(entity) => world
            .get::<RefineryInventory>(entity)
            .map_or(0, |inventory| inventory.output_contents().get(kind)),
    }
}

pub(crate) fn withdraw_source(
    world: &mut World,
    source: StockEndpoint,
    kind: ResourceKind,
    amount: u32,
) -> bool {
    match source {
        StockEndpoint::NaturalNode(_) => false,
        StockEndpoint::CarriedResource(entity) => world
            .get_mut::<CarriedResource>(entity)
            .is_some_and(|mut inventory| inventory.consume(kind, amount)),
        StockEndpoint::Warehouse(entity) => world
            .get_mut::<StorageInventory>(entity)
            .is_some_and(|mut inventory| inventory.consume(kind, amount)),
        StockEndpoint::Farm(entity) => world
            .get_mut::<FarmInventory>(entity)
            .is_some_and(|mut inventory| inventory.consume(kind, amount)),
        StockEndpoint::ForesterLodge(entity) => world
            .get_mut::<ForesterLodgeInventory>(entity)
            .is_some_and(|mut inventory| inventory.consume(kind, amount)),
        StockEndpoint::RefineryInput(entity) => world
            .get_mut::<RefineryInventory>(entity)
            .is_some_and(|mut inventory| inventory.consume_input(kind, amount)),
        StockEndpoint::RefineryOutput(entity) => world
            .get_mut::<RefineryInventory>(entity)
            .is_some_and(|mut inventory| inventory.consume_output(kind, amount)),
    }
}

pub(crate) fn restore_source(
    world: &mut World,
    source: StockEndpoint,
    kind: ResourceKind,
    amount: u32,
) -> bool {
    match source {
        StockEndpoint::Warehouse(entity) => world
            .get_mut::<StorageInventory>(entity)
            .is_some_and(|mut inventory| inventory.add(kind, amount)),
        StockEndpoint::Farm(entity) if kind == ResourceKind::Crops => world
            .get_mut::<FarmInventory>(entity)
            .is_some_and(|mut inventory| inventory.add_crops(amount)),
        StockEndpoint::ForesterLodge(entity) if kind == ResourceKind::Wood => world
            .get_mut::<ForesterLodgeInventory>(entity)
            .is_some_and(|mut inventory| inventory.add_wood(amount)),
        StockEndpoint::RefineryOutput(entity) => {
            let Some(building) = world.get::<Building>(entity).copied() else {
                return false;
            };
            world
                .get_mut::<RefineryInventory>(entity)
                .is_some_and(|mut inventory| inventory.add_output(building.kind, kind, amount))
        }
        _ => false,
    }
}

pub(crate) fn deposit_sink(
    world: &mut World,
    sink: SinkEndpoint,
    kind: ResourceKind,
    amount: u32,
) -> bool {
    match sink {
        SinkEndpoint::Storage(entity) => world
            .get_mut::<StorageInventory>(entity)
            .is_some_and(|mut inventory| inventory.add(kind, amount)),
        SinkEndpoint::RefineryInput(entity) => {
            let Some(building) = world.get::<Building>(entity).copied() else {
                return false;
            };
            world
                .get_mut::<RefineryInventory>(entity)
                .is_some_and(|mut inventory| inventory.add_input(building.kind, kind, amount))
        }
        _ => false,
    }
}

pub(crate) fn sink_can_accept(
    world: &World,
    sink: SinkEndpoint,
    kind: ResourceKind,
    amount: u32,
) -> bool {
    match sink {
        SinkEndpoint::Storage(entity) => {
            building_is_active(world, entity)
                && world
                    .get::<StorageInventory>(entity)
                    .is_some_and(|inventory| {
                        inventory.is_allowed(kind) && inventory.free_size() >= amount
                    })
        }
        SinkEndpoint::RefineryInput(entity) => {
            building_is_active(world, entity)
                && world.get::<Building>(entity).is_some_and(|building| {
                    recipes_for_building(building.kind)
                        .iter()
                        .any(|recipe| recipe.definition().input() == kind)
                })
                && world
                    .get::<RefineryInventory>(entity)
                    .is_some_and(|inventory| inventory.input_free_size() >= amount)
        }
        _ => false,
    }
}

pub(crate) fn sink_interaction_cells(
    world: &World,
    snapshot: &NavigationSnapshot,
    sink: SinkEndpoint,
) -> Vec<CellCoord> {
    world
        .get::<Building>(sink.entity())
        .map(|building| snapshot.exterior_interaction_cells(building.footprint))
        .unwrap_or_default()
}

pub(crate) fn building_is_active(world: &World, entity: Entity) -> bool {
    world
        .get::<BuildingActivity>(entity)
        .is_none_or(|activity| activity.is_active())
}

pub(crate) fn source_is_active(world: &World, source: StockEndpoint) -> bool {
    match source {
        StockEndpoint::Warehouse(entity)
        | StockEndpoint::RefineryInput(entity)
        | StockEndpoint::RefineryOutput(entity) => building_is_active(world, entity),
        _ => true,
    }
}

fn endpoint_order(endpoint: StockEndpoint) -> u8 {
    match endpoint {
        StockEndpoint::NaturalNode(_) => 0,
        StockEndpoint::CarriedResource(_) => 1,
        StockEndpoint::Warehouse(_) => 2,
        StockEndpoint::Farm(_) => 3,
        StockEndpoint::ForesterLodge(_) => 4,
        StockEndpoint::RefineryInput(_) => 5,
        StockEndpoint::RefineryOutput(_) => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingFootprint, BuildingKind};
    use crate::grid::{Grid, GridSize};
    use crate::tile::{TileBundle, TileIndex};

    #[test]
    fn endpoint_order_uses_the_stable_variant_rank() {
        let entity = Entity::PLACEHOLDER;
        let endpoints = [
            StockEndpoint::NaturalNode(entity),
            StockEndpoint::CarriedResource(entity),
            StockEndpoint::Warehouse(entity),
            StockEndpoint::Farm(entity),
            StockEndpoint::ForesterLodge(entity),
            StockEndpoint::RefineryInput(entity),
            StockEndpoint::RefineryOutput(entity),
        ];

        assert_eq!(endpoints.map(endpoint_order), [0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn stock_sources_discover_filter_exclude_and_sort_endpoints() {
        let mut world = World::new();
        let worker = world
            .spawn((
                CarriedResource::of(ResourceKind::Wood, 1),
                inactive_activity(),
            ))
            .id();
        let combined = world
            .spawn((
                ResourceNode {
                    kind: ResourceKind::Wood,
                    quantity: 2,
                },
                storage_inventory(ResourceKind::Wood, 3),
                forester_inventory(4),
            ))
            .id();
        let natural = world
            .spawn(ResourceNode {
                kind: ResourceKind::Wood,
                quantity: 5,
            })
            .id();
        let refinery_input = world
            .spawn(refinery_inventory(
                BuildingKind::Sawmill,
                Some((ResourceKind::Wood, 6)),
                None,
            ))
            .id();
        let farm = world.spawn((farm_inventory(7), inactive_activity())).id();
        let refinery_output = world
            .spawn(refinery_inventory(
                BuildingKind::Sawmill,
                None,
                Some((ResourceKind::Planks, 8)),
            ))
            .id();
        let inactive = world
            .spawn((
                ResourceNode {
                    kind: ResourceKind::Wood,
                    quantity: 9,
                },
                storage_inventory(ResourceKind::Wood, 10),
                forester_inventory(11),
                refinery_inventory(
                    BuildingKind::Sawmill,
                    Some((ResourceKind::Wood, 12)),
                    Some((ResourceKind::Planks, 12)),
                ),
                inactive_activity(),
            ))
            .id();
        let excluded = world
            .spawn((
                ResourceNode {
                    kind: ResourceKind::Wood,
                    quantity: 13,
                },
                storage_inventory(ResourceKind::Wood, 14),
                forester_inventory(15),
                refinery_inventory(BuildingKind::Sawmill, Some((ResourceKind::Wood, 16)), None),
            ))
            .id();
        world.spawn(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 0,
        });
        world.spawn(ResourceNode {
            kind: ResourceKind::Stone,
            quantity: 1,
        });
        world.spawn(StorageInventory::for_kind(BuildingKind::Depot));
        world.spawn(ForesterLodgeInventory::empty());
        world.spawn(RefineryInventory::empty());

        let mut expected_by_entity = [
            (worker, vec![StockEndpoint::CarriedResource(worker)]),
            (
                combined,
                vec![
                    StockEndpoint::NaturalNode(combined),
                    StockEndpoint::Warehouse(combined),
                    StockEndpoint::ForesterLodge(combined),
                ],
            ),
            (natural, vec![StockEndpoint::NaturalNode(natural)]),
            (
                refinery_input,
                vec![StockEndpoint::RefineryInput(refinery_input)],
            ),
            (
                inactive,
                vec![
                    StockEndpoint::NaturalNode(inactive),
                    StockEndpoint::ForesterLodge(inactive),
                ],
            ),
        ];
        expected_by_entity.sort_unstable_by_key(|(entity, _)| entity.to_bits());
        let expected = expected_by_entity
            .into_iter()
            .flat_map(|(_, endpoints)| endpoints)
            .collect::<Vec<_>>();
        assert_eq!(
            stock_sources(&mut world, ResourceKind::Wood, excluded, worker),
            expected
        );
        assert_eq!(
            stock_sources(&mut world, ResourceKind::Crops, excluded, worker),
            vec![StockEndpoint::Farm(farm)]
        );
        assert!(stock_sources(&mut world, ResourceKind::Crops, farm, worker).is_empty());
        assert_eq!(
            stock_sources(&mut world, ResourceKind::Planks, excluded, worker),
            vec![StockEndpoint::RefineryOutput(refinery_output)]
        );

        let worker_excluded = stock_sources(&mut world, ResourceKind::Wood, worker, worker);
        assert!(worker_excluded.contains(&StockEndpoint::CarriedResource(worker)));
    }

    #[test]
    fn source_stock_and_withdraw_dispatch_all_seven_variants() {
        let mut world = World::new();
        let natural = world
            .spawn(ResourceNode {
                kind: ResourceKind::Wood,
                quantity: 5,
            })
            .id();
        let carried = world.spawn(CarriedResource::of(ResourceKind::Wood, 5)).id();
        let storage = world.spawn(storage_inventory(ResourceKind::Wood, 5)).id();
        let farm = world.spawn(farm_inventory(5)).id();
        let lodge = world.spawn(forester_inventory(5)).id();
        let refinery = world
            .spawn(refinery_inventory(
                BuildingKind::Sawmill,
                Some((ResourceKind::Wood, 5)),
                Some((ResourceKind::Planks, 5)),
            ))
            .id();
        let cases = [
            (
                StockEndpoint::NaturalNode(natural),
                ResourceKind::Wood,
                false,
            ),
            (
                StockEndpoint::CarriedResource(carried),
                ResourceKind::Wood,
                true,
            ),
            (StockEndpoint::Warehouse(storage), ResourceKind::Wood, true),
            (StockEndpoint::Farm(farm), ResourceKind::Crops, true),
            (
                StockEndpoint::ForesterLodge(lodge),
                ResourceKind::Wood,
                true,
            ),
            (
                StockEndpoint::RefineryInput(refinery),
                ResourceKind::Wood,
                true,
            ),
            (
                StockEndpoint::RefineryOutput(refinery),
                ResourceKind::Planks,
                true,
            ),
        ];

        for (source, kind, mutable) in cases {
            assert_eq!(source_stock(&world, source, kind), 5);
            assert_eq!(withdraw_source(&mut world, source, kind, 2), mutable);
            let remaining = if mutable { 3 } else { 5 };
            assert_eq!(source_stock(&world, source, kind), remaining);
            assert!(!withdraw_source(&mut world, source, kind, 4));
            assert_eq!(source_stock(&world, source, kind), remaining);
            assert_eq!(source_stock(&world, source, ResourceKind::Gold), 0);
            assert!(!withdraw_source(&mut world, source, ResourceKind::Gold, 1));
        }

        let missing = world.spawn_empty().id();
        for source in all_stock_endpoints(missing) {
            assert_eq!(source_stock(&world, source, ResourceKind::Wood), 0);
            assert!(!withdraw_source(&mut world, source, ResourceKind::Wood, 1));
        }
    }

    #[test]
    fn activity_checks_preserve_missing_activity_and_source_asymmetry() {
        let mut world = World::new();
        let missing = world.spawn_empty().id();
        let active = world.spawn(BuildingActivity::active()).id();
        let inactive = world.spawn(inactive_activity()).id();

        assert!(building_is_active(&world, missing));
        assert!(building_is_active(&world, active));
        assert!(!building_is_active(&world, inactive));

        let expected = [true, true, false, true, true, false, false];
        for (source, expected) in all_stock_endpoints(inactive).into_iter().zip(expected) {
            assert_eq!(source_is_active(&world, source), expected, "{source:?}");
        }
        for source in [
            StockEndpoint::Warehouse(missing),
            StockEndpoint::RefineryInput(missing),
            StockEndpoint::RefineryOutput(missing),
        ] {
            assert!(source_is_active(&world, source));
        }

        let inactive_storage = world
            .spawn((
                storage_inventory(ResourceKind::Wood, 2),
                inactive_activity(),
            ))
            .id();
        let inactive_refinery = world
            .spawn((
                refinery_inventory(
                    BuildingKind::Sawmill,
                    Some((ResourceKind::Wood, 2)),
                    Some((ResourceKind::Planks, 2)),
                ),
                inactive_activity(),
            ))
            .id();
        for (source, kind) in [
            (
                StockEndpoint::Warehouse(inactive_storage),
                ResourceKind::Wood,
            ),
            (
                StockEndpoint::RefineryInput(inactive_refinery),
                ResourceKind::Wood,
            ),
            (
                StockEndpoint::RefineryOutput(inactive_refinery),
                ResourceKind::Planks,
            ),
        ] {
            assert!(!source_is_active(&world, source));
            assert!(withdraw_source(&mut world, source, kind, 1));
            assert_eq!(source_stock(&world, source, kind), 1);
        }
    }

    #[test]
    fn source_interaction_cells_preserve_geometry_shortcuts_and_missing_components() {
        let mut world = navigation_world();
        let natural = world
            .spawn((
                TilePosition {
                    coord: CellCoord::new(1, 1),
                },
                ResourceNode {
                    kind: ResourceKind::Stone,
                    quantity: 1,
                },
            ))
            .id();
        let worker = world.spawn(NpcPosition::new(CellCoord::new(6, 5))).id();
        let building = world
            .spawn(Building::new(
                BuildingKind::Depot,
                BuildingFootprint::new(CellCoord::new(3, 3), 2, 2),
            ))
            .id();
        let missing = world.spawn_empty().id();
        let worker_without_position = world
            .spawn(Building::new(
                BuildingKind::Depot,
                BuildingFootprint::new(CellCoord::new(0, 6), 1, 1),
            ))
            .id();
        let snapshot = NavigationSnapshot::from_world(&world).unwrap();
        let exterior = building_exterior_cells();

        assert_eq!(
            source_interaction_cells(
                &world,
                &snapshot,
                StockEndpoint::NaturalNode(natural),
                worker,
            ),
            vec![
                CellCoord::new(1, 0),
                CellCoord::new(0, 1),
                CellCoord::new(2, 1),
                CellCoord::new(1, 2),
            ]
        );
        assert_eq!(
            source_interaction_cells(
                &world,
                &snapshot,
                StockEndpoint::CarriedResource(worker),
                worker,
            ),
            vec![CellCoord::new(6, 5)]
        );

        for source in [
            StockEndpoint::CarriedResource(building),
            StockEndpoint::Warehouse(building),
            StockEndpoint::Farm(building),
            StockEndpoint::ForesterLodge(building),
            StockEndpoint::RefineryInput(building),
            StockEndpoint::RefineryOutput(building),
        ] {
            assert_eq!(
                source_interaction_cells(&world, &snapshot, source, worker),
                exterior,
                "{source:?}"
            );
        }

        assert!(source_interaction_cells(
            &world,
            &snapshot,
            StockEndpoint::NaturalNode(missing),
            worker,
        )
        .is_empty());
        for source in [
            StockEndpoint::CarriedResource(missing),
            StockEndpoint::Warehouse(missing),
            StockEndpoint::Farm(missing),
            StockEndpoint::ForesterLodge(missing),
            StockEndpoint::RefineryInput(missing),
            StockEndpoint::RefineryOutput(missing),
        ] {
            assert!(source_interaction_cells(&world, &snapshot, source, worker).is_empty());
        }
        assert!(source_interaction_cells(
            &world,
            &snapshot,
            StockEndpoint::CarriedResource(worker_without_position),
            worker_without_position,
        )
        .is_empty());
    }

    #[test]
    fn sink_interaction_cells_are_component_driven_for_every_variant() {
        let mut world = navigation_world();
        let building = world
            .spawn(Building::new(
                BuildingKind::Depot,
                BuildingFootprint::new(CellCoord::new(3, 3), 2, 2),
            ))
            .id();
        let missing = world.spawn_empty().id();
        let snapshot = NavigationSnapshot::from_world(&world).unwrap();
        let expected = building_exterior_cells();

        for sink in all_sink_endpoints(building) {
            assert_eq!(
                sink_interaction_cells(&world, &snapshot, sink),
                expected,
                "{sink:?}"
            );
        }
        for sink in all_sink_endpoints(missing) {
            assert!(sink_interaction_cells(&world, &snapshot, sink).is_empty());
        }
    }

    #[test]
    fn restore_source_supports_only_the_existing_combinations() {
        let mut world = World::new();
        let storage = world
            .spawn((
                StorageInventory::for_kind(BuildingKind::Depot),
                inactive_activity(),
            ))
            .id();
        let farm = world
            .spawn((FarmInventory::empty(), inactive_activity()))
            .id();
        let lodge = world
            .spawn((ForesterLodgeInventory::empty(), inactive_activity()))
            .id();
        let refinery = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                RefineryInventory::empty(),
                inactive_activity(),
            ))
            .id();

        assert!(restore_source(
            &mut world,
            StockEndpoint::Warehouse(storage),
            ResourceKind::Wood,
            2,
        ));
        assert!(restore_source(
            &mut world,
            StockEndpoint::Farm(farm),
            ResourceKind::Crops,
            3,
        ));
        assert!(restore_source(
            &mut world,
            StockEndpoint::ForesterLodge(lodge),
            ResourceKind::Wood,
            4,
        ));
        assert!(restore_source(
            &mut world,
            StockEndpoint::RefineryOutput(refinery),
            ResourceKind::Planks,
            5,
        ));
        assert_eq!(
            source_stock(
                &world,
                StockEndpoint::Warehouse(storage),
                ResourceKind::Wood
            ),
            2
        );
        assert_eq!(
            source_stock(&world, StockEndpoint::Farm(farm), ResourceKind::Crops),
            3
        );
        assert_eq!(
            source_stock(
                &world,
                StockEndpoint::ForesterLodge(lodge),
                ResourceKind::Wood,
            ),
            4
        );
        assert_eq!(
            source_stock(
                &world,
                StockEndpoint::RefineryOutput(refinery),
                ResourceKind::Planks,
            ),
            5
        );

        let natural = world
            .spawn(ResourceNode {
                kind: ResourceKind::Wood,
                quantity: 1,
            })
            .id();
        let carried = world.spawn(CarriedResource::empty()).id();
        assert!(!restore_source(
            &mut world,
            StockEndpoint::NaturalNode(natural),
            ResourceKind::Wood,
            1,
        ));
        assert!(!restore_source(
            &mut world,
            StockEndpoint::CarriedResource(carried),
            ResourceKind::Wood,
            1,
        ));
        assert!(!restore_source(
            &mut world,
            StockEndpoint::RefineryInput(refinery),
            ResourceKind::Wood,
            1,
        ));
        assert!(!restore_source(
            &mut world,
            StockEndpoint::Farm(farm),
            ResourceKind::Wood,
            1,
        ));
        assert!(!restore_source(
            &mut world,
            StockEndpoint::ForesterLodge(lodge),
            ResourceKind::Crops,
            1,
        ));
        assert!(!restore_source(
            &mut world,
            StockEndpoint::RefineryOutput(refinery),
            ResourceKind::Wood,
            1,
        ));
        assert_eq!(world.get::<ResourceNode>(natural).unwrap().quantity, 1);
        assert_eq!(
            world
                .get::<CarriedResource>(carried)
                .unwrap()
                .contents()
                .get(ResourceKind::Wood),
            0
        );
        assert_eq!(world.get::<FarmInventory>(farm).unwrap().crops(), 3);
        assert_eq!(
            world.get::<ForesterLodgeInventory>(lodge).unwrap().wood(),
            4
        );
        assert_eq!(
            source_stock(
                &world,
                StockEndpoint::RefineryInput(refinery),
                ResourceKind::Wood,
            ),
            0
        );
        assert_eq!(
            source_stock(
                &world,
                StockEndpoint::RefineryOutput(refinery),
                ResourceKind::Planks,
            ),
            5
        );

        let missing = world.spawn_empty().id();
        assert!(!restore_source(
            &mut world,
            StockEndpoint::Warehouse(missing),
            ResourceKind::Wood,
            1,
        ));
        assert!(!restore_source(
            &mut world,
            StockEndpoint::Farm(missing),
            ResourceKind::Crops,
            1,
        ));
        assert!(!restore_source(
            &mut world,
            StockEndpoint::ForesterLodge(missing),
            ResourceKind::Wood,
            1,
        ));
        assert!(!restore_source(
            &mut world,
            StockEndpoint::RefineryOutput(missing),
            ResourceKind::Planks,
            1,
        ));

        let missing_building = world.spawn(RefineryInventory::empty()).id();
        let missing_inventory = world
            .spawn(Building::new(
                BuildingKind::Sawmill,
                BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
            ))
            .id();
        let wrong_building = world
            .spawn((
                Building::new(
                    BuildingKind::Depot,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                RefineryInventory::empty(),
            ))
            .id();
        for entity in [missing_building, missing_inventory, wrong_building] {
            assert!(!restore_source(
                &mut world,
                StockEndpoint::RefineryOutput(entity),
                ResourceKind::Planks,
                1,
            ));
        }

        let mut filtered_inventory = StorageInventory::for_kind(BuildingKind::Depot);
        filtered_inventory.set_allowed(ResourceKind::Wood, false);
        let filtered = world.spawn(filtered_inventory).id();
        assert!(!restore_source(
            &mut world,
            StockEndpoint::Warehouse(filtered),
            ResourceKind::Wood,
            1,
        ));

        let mut full_storage = StorageInventory::for_kind(BuildingKind::Depot);
        assert!(full_storage.add(ResourceKind::Stone, full_storage.free_size()));
        let full_storage = world.spawn(full_storage).id();
        let mut full_farm = FarmInventory::empty();
        assert!(full_farm.add_crops(full_farm.free_size()));
        let full_farm = world.spawn(full_farm).id();
        let mut full_lodge = ForesterLodgeInventory::empty();
        assert!(full_lodge.add_wood(full_lodge.free_size()));
        let full_lodge = world.spawn(full_lodge).id();
        let mut full_refinery = RefineryInventory::empty();
        assert!(full_refinery.add_output(
            BuildingKind::Sawmill,
            ResourceKind::Planks,
            full_refinery.output_free_size(),
        ));
        let full_refinery = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                full_refinery,
            ))
            .id();
        for (source, kind) in [
            (StockEndpoint::Warehouse(full_storage), ResourceKind::Wood),
            (StockEndpoint::Farm(full_farm), ResourceKind::Crops),
            (StockEndpoint::ForesterLodge(full_lodge), ResourceKind::Wood),
            (
                StockEndpoint::RefineryOutput(full_refinery),
                ResourceKind::Planks,
            ),
        ] {
            assert!(!restore_source(&mut world, source, kind, 1));
        }
    }

    #[test]
    fn storage_sink_acceptance_and_deposit_honor_activity_filter_and_capacity() {
        let mut world = World::new();
        let missing_activity = world
            .spawn(StorageInventory::for_kind(BuildingKind::Depot))
            .id();
        let active = world
            .spawn((
                StorageInventory::for_kind(BuildingKind::Depot),
                BuildingActivity::active(),
            ))
            .id();
        let inactive = world
            .spawn((
                StorageInventory::for_kind(BuildingKind::Depot),
                inactive_activity(),
            ))
            .id();
        let mut filtered_inventory = StorageInventory::for_kind(BuildingKind::Depot);
        filtered_inventory.set_allowed(ResourceKind::Wood, false);
        let filtered = world.spawn(filtered_inventory).id();
        let mut nearly_full_inventory = StorageInventory::for_kind(BuildingKind::Depot);
        let remaining_one = nearly_full_inventory.free_size() - 1;
        assert!(nearly_full_inventory.add(ResourceKind::Stone, remaining_one));
        let nearly_full = world.spawn(nearly_full_inventory).id();
        let missing_inventory = world.spawn_empty().id();

        for entity in [missing_activity, active] {
            assert!(sink_can_accept(
                &world,
                SinkEndpoint::Storage(entity),
                ResourceKind::Wood,
                2,
            ));
            assert!(deposit_sink(
                &mut world,
                SinkEndpoint::Storage(entity),
                ResourceKind::Wood,
                2,
            ));
            assert_eq!(
                world
                    .get::<StorageInventory>(entity)
                    .unwrap()
                    .contents()
                    .get(ResourceKind::Wood),
                2
            );
        }
        assert!(!sink_can_accept(
            &world,
            SinkEndpoint::Storage(inactive),
            ResourceKind::Wood,
            2,
        ));
        assert!(deposit_sink(
            &mut world,
            SinkEndpoint::Storage(inactive),
            ResourceKind::Wood,
            2,
        ));
        assert_eq!(
            world
                .get::<StorageInventory>(inactive)
                .unwrap()
                .contents()
                .get(ResourceKind::Wood),
            2
        );

        assert!(!sink_can_accept(
            &world,
            SinkEndpoint::Storage(filtered),
            ResourceKind::Wood,
            1,
        ));
        assert!(!deposit_sink(
            &mut world,
            SinkEndpoint::Storage(filtered),
            ResourceKind::Wood,
            1,
        ));
        assert_eq!(
            world
                .get::<StorageInventory>(filtered)
                .unwrap()
                .contents()
                .get(ResourceKind::Wood),
            0
        );
        assert!(!sink_can_accept(
            &world,
            SinkEndpoint::Storage(nearly_full),
            ResourceKind::Wood,
            2,
        ));
        assert!(!deposit_sink(
            &mut world,
            SinkEndpoint::Storage(nearly_full),
            ResourceKind::Wood,
            2,
        ));
        assert_eq!(
            world
                .get::<StorageInventory>(nearly_full)
                .unwrap()
                .contents()
                .get(ResourceKind::Wood),
            0
        );
        assert!(sink_can_accept(
            &world,
            SinkEndpoint::Storage(nearly_full),
            ResourceKind::Wood,
            1,
        ));
        assert!(deposit_sink(
            &mut world,
            SinkEndpoint::Storage(nearly_full),
            ResourceKind::Wood,
            1,
        ));
        assert_eq!(
            world
                .get::<StorageInventory>(nearly_full)
                .unwrap()
                .contents()
                .get(ResourceKind::Wood),
            1
        );
        assert!(!sink_can_accept(
            &world,
            SinkEndpoint::Storage(missing_inventory),
            ResourceKind::Wood,
            1,
        ));
        assert!(!deposit_sink(
            &mut world,
            SinkEndpoint::Storage(missing_inventory),
            ResourceKind::Wood,
            1,
        ));

        for sink in [
            SinkEndpoint::Blueprint(active),
            SinkEndpoint::FoodPouch(active),
            SinkEndpoint::RefineryOutput(active),
        ] {
            assert!(!sink_can_accept(&world, sink, ResourceKind::Wood, 1));
            assert!(!deposit_sink(&mut world, sink, ResourceKind::Wood, 1));
        }
    }

    #[test]
    fn refinery_sink_acceptance_and_deposit_honor_recipes_capacity_and_activity() {
        let mut world = World::new();
        for (building_kind, resource_kind) in [
            (BuildingKind::Sawmill, ResourceKind::Wood),
            (BuildingKind::Stoneworks, ResourceKind::Stone),
            (BuildingKind::Kitchen, ResourceKind::Crops),
            (BuildingKind::Kitchen, ResourceKind::WildBerries),
        ] {
            let refinery = world
                .spawn((
                    Building::new(
                        building_kind,
                        BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                    ),
                    RefineryInventory::empty(),
                ))
                .id();
            assert!(sink_can_accept(
                &world,
                SinkEndpoint::RefineryInput(refinery),
                resource_kind,
                2,
            ));
            assert!(deposit_sink(
                &mut world,
                SinkEndpoint::RefineryInput(refinery),
                resource_kind,
                2,
            ));
            assert_eq!(
                source_stock(
                    &world,
                    StockEndpoint::RefineryInput(refinery),
                    resource_kind,
                ),
                2
            );
        }

        let active = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                RefineryInventory::empty(),
                BuildingActivity::active(),
            ))
            .id();
        assert!(sink_can_accept(
            &world,
            SinkEndpoint::RefineryInput(active),
            ResourceKind::Wood,
            1,
        ));

        let inactive = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                RefineryInventory::empty(),
                inactive_activity(),
            ))
            .id();
        assert!(!sink_can_accept(
            &world,
            SinkEndpoint::RefineryInput(inactive),
            ResourceKind::Wood,
            1,
        ));
        assert!(deposit_sink(
            &mut world,
            SinkEndpoint::RefineryInput(inactive),
            ResourceKind::Wood,
            1,
        ));
        assert_eq!(
            source_stock(
                &world,
                StockEndpoint::RefineryInput(inactive),
                ResourceKind::Wood,
            ),
            1
        );

        let wrong_recipe = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                RefineryInventory::empty(),
            ))
            .id();
        let non_refinery = world
            .spawn((
                Building::new(
                    BuildingKind::Depot,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                RefineryInventory::empty(),
            ))
            .id();
        for (entity, kind) in [
            (wrong_recipe, ResourceKind::Stone),
            (non_refinery, ResourceKind::Wood),
        ] {
            assert!(!sink_can_accept(
                &world,
                SinkEndpoint::RefineryInput(entity),
                kind,
                1,
            ));
            assert!(!deposit_sink(
                &mut world,
                SinkEndpoint::RefineryInput(entity),
                kind,
                1,
            ));
            assert_eq!(
                source_stock(&world, StockEndpoint::RefineryInput(entity), kind),
                0
            );
        }

        let missing_building = world.spawn(RefineryInventory::empty()).id();
        let missing_inventory = world
            .spawn(Building::new(
                BuildingKind::Sawmill,
                BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
            ))
            .id();
        for entity in [missing_building, missing_inventory] {
            assert!(!sink_can_accept(
                &world,
                SinkEndpoint::RefineryInput(entity),
                ResourceKind::Wood,
                1,
            ));
            assert!(!deposit_sink(
                &mut world,
                SinkEndpoint::RefineryInput(entity),
                ResourceKind::Wood,
                1,
            ));
        }

        let mut full_inventory = RefineryInventory::empty();
        assert!(full_inventory.add_input(
            BuildingKind::Sawmill,
            ResourceKind::Wood,
            full_inventory.input_free_size(),
        ));
        let full = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                full_inventory,
            ))
            .id();
        assert!(!sink_can_accept(
            &world,
            SinkEndpoint::RefineryInput(full),
            ResourceKind::Wood,
            1,
        ));
        assert!(!deposit_sink(
            &mut world,
            SinkEndpoint::RefineryInput(full),
            ResourceKind::Wood,
            1,
        ));

        let mut nearly_full_inventory = RefineryInventory::empty();
        let remaining_one = nearly_full_inventory.input_free_size() - 1;
        assert!(nearly_full_inventory.add_input(
            BuildingKind::Sawmill,
            ResourceKind::Wood,
            remaining_one,
        ));
        let nearly_full = world
            .spawn((
                Building::new(
                    BuildingKind::Sawmill,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                nearly_full_inventory,
            ))
            .id();
        assert!(!sink_can_accept(
            &world,
            SinkEndpoint::RefineryInput(nearly_full),
            ResourceKind::Wood,
            2,
        ));
        assert!(!deposit_sink(
            &mut world,
            SinkEndpoint::RefineryInput(nearly_full),
            ResourceKind::Wood,
            2,
        ));
        assert!(sink_can_accept(
            &world,
            SinkEndpoint::RefineryInput(nearly_full),
            ResourceKind::Wood,
            1,
        ));
        assert!(deposit_sink(
            &mut world,
            SinkEndpoint::RefineryInput(nearly_full),
            ResourceKind::Wood,
            1,
        ));
        assert_eq!(
            source_stock(
                &world,
                StockEndpoint::RefineryInput(nearly_full),
                ResourceKind::Wood,
            ),
            remaining_one + 1
        );
    }

    fn all_stock_endpoints(entity: Entity) -> [StockEndpoint; 7] {
        [
            StockEndpoint::NaturalNode(entity),
            StockEndpoint::CarriedResource(entity),
            StockEndpoint::Warehouse(entity),
            StockEndpoint::Farm(entity),
            StockEndpoint::ForesterLodge(entity),
            StockEndpoint::RefineryInput(entity),
            StockEndpoint::RefineryOutput(entity),
        ]
    }

    fn all_sink_endpoints(entity: Entity) -> [SinkEndpoint; 5] {
        [
            SinkEndpoint::Blueprint(entity),
            SinkEndpoint::FoodPouch(entity),
            SinkEndpoint::Storage(entity),
            SinkEndpoint::RefineryInput(entity),
            SinkEndpoint::RefineryOutput(entity),
        ]
    }

    fn storage_inventory(kind: ResourceKind, amount: u32) -> StorageInventory {
        let mut inventory = StorageInventory::for_kind(BuildingKind::Depot);
        assert!(inventory.add(kind, amount));
        inventory
    }

    fn farm_inventory(crops: u32) -> FarmInventory {
        let mut inventory = FarmInventory::empty();
        assert!(inventory.add_crops(crops));
        inventory
    }

    fn forester_inventory(wood: u32) -> ForesterLodgeInventory {
        let mut inventory = ForesterLodgeInventory::empty();
        assert!(inventory.add_wood(wood));
        inventory
    }

    fn refinery_inventory(
        building: BuildingKind,
        input: Option<(ResourceKind, u32)>,
        output: Option<(ResourceKind, u32)>,
    ) -> RefineryInventory {
        let mut inventory = RefineryInventory::empty();
        if let Some((kind, amount)) = input {
            assert!(inventory.add_input(building, kind, amount));
        }
        if let Some((kind, amount)) = output {
            assert!(inventory.add_output(building, kind, amount));
        }
        inventory
    }

    fn inactive_activity() -> BuildingActivity {
        let mut activity = BuildingActivity::active();
        activity.set_active(false);
        activity
    }

    fn navigation_world() -> World {
        let size = GridSize::new(8, 8);
        let mut world = World::new();
        world.insert_resource(Grid::new(size.width(), size.height()));
        let mut index = TileIndex::new(size);
        for coord in size.iter_coords() {
            let tile = world.spawn(TileBundle::new(coord)).id();
            assert!(index.set(coord, tile));
        }
        world.insert_resource(index);
        world
    }

    fn building_exterior_cells() -> Vec<CellCoord> {
        vec![
            CellCoord::new(3, 2),
            CellCoord::new(4, 2),
            CellCoord::new(2, 3),
            CellCoord::new(5, 3),
            CellCoord::new(2, 4),
            CellCoord::new(5, 4),
            CellCoord::new(3, 5),
            CellCoord::new(4, 5),
        ]
    }
}
