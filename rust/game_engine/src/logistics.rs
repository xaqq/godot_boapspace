use bevy_ecs::prelude::*;

use crate::ai::{
    AiConstructBuilding, AiGatherResource, AiIdleRoam, AiSearchForFood,
    CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE, DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD,
    DEFAULT_NPC_FOOD_INVENTORY_TARGET, RESOURCE_GATHER_TICKS_PER_UNIT,
};
use crate::buildings::{Building, BuildingBlueprint, ConstructionProgress};
use crate::components::{
    AiKeepEnoughFoodInInventory, MovementTarget, Npc, NpcInventory, NpcPosition, ResourceNode,
    TilePosition,
};
use crate::farming::{AiHarvestField, AiSeedField};
use crate::forestry::{AiCutTreePlot, AiSeedTreePlot};
use crate::navigation::{
    current_navigation_snapshot, refresh_navigation_snapshot_cells, NavigationDistances,
    NavigationSnapshot, NpcRoute,
};
use crate::refining::{
    endpoint_entity, source_interaction_cells, source_stock, stock_sources, withdraw_source,
    AiRefineResource, RefineryInventory, Reservation, ReservationLedger, SinkEndpoint,
    StockEndpoint,
};
use crate::resources::ResourceKind;
use crate::skills::{NpcSkills, SkillKind};

const NATURAL_RESOURCE_CONSTRUCTION_BATCH_SIZE: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiFoodHaul {
    source: StockEndpoint,
    amount: u32,
}

impl AiFoodHaul {
    pub const fn source(self) -> StockEndpoint {
        self.source
    }
    pub const fn amount(self) -> u32 {
        self.amount
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionHaulPhase {
    ToSource,
    Gathering { progress_ticks: u32 },
    ToBlueprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiConstructionHaul {
    blueprint: Entity,
    source: Option<StockEndpoint>,
    kind: ResourceKind,
    amount: u32,
    phase: ConstructionHaulPhase,
}

impl AiConstructionHaul {
    pub const fn blueprint(self) -> Entity {
        self.blueprint
    }
    pub const fn source(self) -> Option<StockEndpoint> {
        self.source
    }
    pub const fn kind(self) -> ResourceKind {
        self.kind
    }
    pub const fn amount(self) -> u32 {
        self.amount
    }
    pub const fn phase(self) -> ConstructionHaulPhase {
        self.phase
    }
}

/// Urgent Food hauling. Sources are intentionally limited to Warehouses and
/// Kitchen output buffers; raw Crops and Wild Berries are never candidates.
pub fn manage_food_logistics(world: &mut World) {
    if world.get_resource::<ReservationLedger>().is_none() {
        world.insert_resource(ReservationLedger::default());
    }
    let Some(snapshot) = current_navigation_snapshot(world) else {
        return;
    };
    let food_sources = food_sources(world);
    let mut query = world.query_filtered::<(
        Entity,
        &NpcPosition,
        &NpcInventory,
        Option<&AiKeepEnoughFoodInInventory>,
        Option<&AiSearchForFood>,
        Option<&AiFoodHaul>,
    ), With<Npc>>();
    let mut npcs = query
        .iter(world)
        .map(|(entity, position, inventory, goal, search, haul)| {
            (
                entity,
                *position,
                *inventory,
                goal.copied(),
                search.is_some(),
                haul.copied(),
            )
        })
        .collect::<Vec<_>>();
    npcs.sort_unstable_by_key(|(entity, ..)| entity.to_bits());

    for (npc, position, inventory, goal, searching, haul) in npcs {
        let start = goal.map_or(DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD, |goal| {
            goal.start_threshold()
        });
        let target = goal.map_or(DEFAULT_NPC_FOOD_INVENTORY_TARGET, |goal| goal.target());
        let carried = inventory.contents().get(ResourceKind::Food);
        if searching && (carried >= target || inventory.free_size() == 0) {
            clear_food_haul(world, npc);
            continue;
        }
        if !searching && carried > start {
            continue;
        }

        let current = haul.filter(|haul| food_source_available(world, haul.source) >= haul.amount);
        let selected = current.or_else(|| {
            choose_food_source(
                world,
                &snapshot,
                &food_sources,
                position.coord,
                npc,
                target.saturating_sub(carried).min(inventory.free_size()),
            )
        });
        let Some(haul) = selected else {
            if searching {
                clear_food_haul(world, npc);
            }
            continue;
        };

        if !searching {
            preempt_lower_priority_work(world, npc);
            world.entity_mut(npc).insert(AiSearchForFood);
            world
                .resource_mut::<ReservationLedger>()
                .release_worker(npc);
            let _ = world
                .resource_mut::<ReservationLedger>()
                .claim(Reservation {
                    worker: npc,
                    source: Some(haul.source),
                    sink: SinkEndpoint::NpcInventory(npc),
                    kind: ResourceKind::Food,
                    amount: haul.amount,
                    task: npc,
                });
            world.entity_mut(npc).insert(haul);
        }

        let goals = source_interaction_cells(world, &snapshot, haul.source, npc);
        if goals.contains(&position.coord) {
            let can_add = world
                .get::<NpcInventory>(npc)
                .is_some_and(|inv| inv.free_size() >= haul.amount);
            if can_add
                && withdraw_source(world, haul.source, ResourceKind::Food, haul.amount)
                && world
                    .get_mut::<NpcInventory>(npc)
                    .is_some_and(|mut inv| inv.add(ResourceKind::Food, haul.amount))
            {
                clear_food_haul(world, npc);
            } else {
                clear_food_haul(world, npc);
            }
        } else if goals.is_empty() {
            clear_food_haul(world, npc);
        } else {
            set_route(world, npc, goals);
        }
    }
}

/// Construction hauling uses the same source endpoint and reservation model as
/// refining. It handles natural gathering one unit at a time, while carried
/// stock and owned inventories use ten-unit batches.
pub fn manage_construction_logistics(world: &mut World) {
    if world.get_resource::<ReservationLedger>().is_none() {
        world.insert_resource(ReservationLedger::default());
    }
    let Some(snapshot) = current_navigation_snapshot(world) else {
        return;
    };

    advance_construction_hauls(world, &snapshot);

    let mut npc_query = world.query_filtered::<(
        Entity,
        &NpcPosition,
        &NpcInventory,
        Option<&AiSearchForFood>,
        Option<&AiConstructBuilding>,
        Option<&AiRefineResource>,
        Option<&AiSeedField>,
        Option<&AiHarvestField>,
        Option<&AiSeedTreePlot>,
        Option<&AiCutTreePlot>,
    ), With<Npc>>();
    let mut npcs = npc_query
        .iter(world)
        .filter(
            |(_, _, _, food, construction, refining, seed, harvest, tree_seed, tree_cut)| {
                food.is_none()
                    && construction.is_none()
                    && refining.is_none()
                    && seed.is_none()
                    && harvest.is_none()
                    && tree_seed.is_none()
                    && tree_cut.is_none()
            },
        )
        .map(|(entity, position, inventory, ..)| (entity, *position, *inventory))
        .collect::<Vec<_>>();
    npcs.sort_unstable_by_key(|(entity, ..)| entity.to_bits());

    let mut blueprint_query = world.query::<(Entity, &BuildingBlueprint, &ConstructionProgress)>();
    let blueprints = blueprint_query
        .iter(world)
        .map(|(entity, blueprint, progress)| (entity, *blueprint, *progress))
        .collect::<Vec<_>>();
    // Construction only looks sources up when the worker is not already carrying
    // the requested kind, so these lists are worker-independent. Populate each
    // kind on first use to keep ticks with no matching demand cheap.
    let mut stock_sources_by_kind: [Option<Vec<StockEndpoint>>; ResourceKind::ALL.len()] =
        std::array::from_fn(|_| None);
    for (npc, position, inventory) in npcs {
        let mut candidates = Vec::new();
        let mut distances = None;
        let mut distances_initialized = false;
        for (blueprint_entity, blueprint, progress) in &blueprints {
            let cost = blueprint.kind.definition().construction_cost();
            for kind in ResourceKind::ALL {
                let reserved = world
                    .resource::<ReservationLedger>()
                    .reserved_to(SinkEndpoint::Blueprint(*blueprint_entity), kind);
                let remaining = progress.remaining(cost, kind).saturating_sub(reserved);
                if remaining == 0 {
                    continue;
                }
                let amount = remaining
                    .min(CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE)
                    .min(inventory.free_size().max(inventory.contents().get(kind)));
                if amount == 0 {
                    continue;
                }
                if inventory.contents().get(kind) > 0 {
                    let goals = blueprint_goals(&snapshot, *blueprint);
                    if let Some(distance) = distance_to_any(
                        &snapshot,
                        position.coord,
                        &mut distances,
                        &mut distances_initialized,
                        goals,
                    ) {
                        candidates.push((
                            distance,
                            blueprint_entity.to_bits(),
                            kind as usize,
                            *blueprint_entity,
                            kind,
                            None,
                            amount.min(inventory.contents().get(kind)),
                        ));
                    }
                    continue;
                }
                let sources = stock_sources_by_kind[kind as usize].get_or_insert_with(|| {
                    stock_sources(world, kind, Entity::PLACEHOLDER, Entity::PLACEHOLDER)
                });
                for &source in sources.iter() {
                    if source_stock(world, source, kind)
                        <= world
                            .resource::<ReservationLedger>()
                            .reserved_from(source, kind)
                    {
                        continue;
                    }
                    let goals = source_interaction_cells(world, &snapshot, source, npc);
                    let Some(distance) = distance_to_any(
                        &snapshot,
                        position.coord,
                        &mut distances,
                        &mut distances_initialized,
                        goals,
                    ) else {
                        continue;
                    };
                    let available = source_stock(world, source, kind).saturating_sub(
                        world
                            .resource::<ReservationLedger>()
                            .reserved_from(source, kind),
                    );
                    let source_batch_size = match source {
                        StockEndpoint::NaturalNode(_) => NATURAL_RESOURCE_CONSTRUCTION_BATCH_SIZE,
                        _ => CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE,
                    };
                    candidates.push((
                        distance,
                        blueprint_entity.to_bits(),
                        kind as usize,
                        *blueprint_entity,
                        kind,
                        Some(source),
                        amount.min(available).min(source_batch_size),
                    ));
                }
            }
        }
        let Some((_, _, _, blueprint, kind, source, amount)) =
            candidates.into_iter().min_by_key(|candidate| {
                (
                    candidate.0,
                    candidate.1,
                    candidate.2,
                    candidate.5.map(endpoint_entity).map_or(0, Entity::to_bits),
                )
            })
        else {
            continue;
        };
        let claim = Reservation {
            worker: npc,
            source,
            sink: SinkEndpoint::Blueprint(blueprint),
            kind,
            amount,
            task: blueprint,
        };
        if !world.resource_mut::<ReservationLedger>().claim(claim) {
            continue;
        }
        let mut construction = AiConstructBuilding::new(blueprint);
        construction.set_target_kind(kind);
        world.entity_mut(npc).insert((
            construction,
            AiConstructionHaul {
                blueprint,
                source,
                kind,
                amount,
                phase: if source.is_some() {
                    ConstructionHaulPhase::ToSource
                } else {
                    ConstructionHaulPhase::ToBlueprint
                },
            },
        ));
    }
}

fn advance_construction_hauls(world: &mut World, snapshot: &NavigationSnapshot) {
    let mut query = world.query::<(Entity, &NpcPosition, &AiConstructionHaul)>();
    let mut jobs = query
        .iter(world)
        .map(|(entity, position, haul)| (entity, *position, *haul))
        .collect::<Vec<_>>();
    jobs.sort_unstable_by_key(|(entity, ..)| entity.to_bits());
    for (npc, position, mut haul) in jobs {
        if world.get::<AiSearchForFood>(npc).is_some() {
            clear_construction_haul(world, npc);
            continue;
        }
        let Some(blueprint) = world.get::<BuildingBlueprint>(haul.blueprint).copied() else {
            clear_construction_haul(world, npc);
            continue;
        };
        match haul.phase {
            ConstructionHaulPhase::ToSource => {
                let Some(source) = haul.source else {
                    haul.phase = ConstructionHaulPhase::ToBlueprint;
                    world.entity_mut(npc).insert(haul);
                    continue;
                };
                let goals = source_interaction_cells(world, snapshot, source, npc);
                if !goals.contains(&position.coord) && source != StockEndpoint::NpcInventory(npc) {
                    if goals.is_empty() {
                        clear_construction_haul(world, npc);
                    } else {
                        set_route(world, npc, goals);
                    }
                    continue;
                }
                if matches!(source, StockEndpoint::NaturalNode(_)) {
                    debug_assert_eq!(haul.amount, NATURAL_RESOURCE_CONSTRUCTION_BATCH_SIZE);
                    haul.phase = ConstructionHaulPhase::Gathering { progress_ticks: 0 };
                } else if world
                    .get::<NpcInventory>(npc)
                    .is_some_and(|inv| inv.free_size() >= haul.amount)
                    && withdraw_source(world, source, haul.kind, haul.amount)
                    && world
                        .get_mut::<NpcInventory>(npc)
                        .is_some_and(|mut inv| inv.add(haul.kind, haul.amount))
                {
                    haul.phase = ConstructionHaulPhase::ToBlueprint;
                } else {
                    clear_construction_haul(world, npc);
                    continue;
                }
                world.entity_mut(npc).insert(haul);
            }
            ConstructionHaulPhase::Gathering { progress_ticks } => {
                let Some(StockEndpoint::NaturalNode(node_entity)) = haul.source else {
                    clear_construction_haul(world, npc);
                    continue;
                };
                let valid = world
                    .get::<ResourceNode>(node_entity)
                    .is_some_and(|node| node.kind == haul.kind && node.quantity > 0);
                if !valid {
                    clear_construction_haul(world, npc);
                    continue;
                }
                let next = progress_ticks.saturating_add(1);
                if next < RESOURCE_GATHER_TICKS_PER_UNIT {
                    haul.phase = ConstructionHaulPhase::Gathering {
                        progress_ticks: next,
                    };
                    world.entity_mut(npc).insert(haul);
                    continue;
                }
                if !world
                    .get_mut::<NpcInventory>(npc)
                    .is_some_and(|mut inv| inv.add(haul.kind, 1))
                {
                    clear_construction_haul(world, npc);
                    continue;
                }
                let depleted = if let Some(mut node) = world.get_mut::<ResourceNode>(node_entity) {
                    node.quantity = node.quantity.saturating_sub(1);
                    node.quantity == 0
                } else {
                    false
                };
                if depleted {
                    let coord = world
                        .get::<TilePosition>(node_entity)
                        .map(|position| position.coord);
                    world.entity_mut(node_entity).remove::<ResourceNode>();
                    if let Some(coord) = coord {
                        refresh_navigation_snapshot_cells(world, [coord]);
                    }
                }
                if let Some(mut skills) = world.get_mut::<NpcSkills>(npc) {
                    if let Some(skill) = SkillKind::try_for_gathered_resource(haul.kind) {
                        skills.add_xp(skill, 1);
                    }
                }
                haul.phase = ConstructionHaulPhase::ToBlueprint;
                world.entity_mut(npc).insert(haul);
            }
            ConstructionHaulPhase::ToBlueprint => {
                let goals = blueprint_goals(snapshot, blueprint);
                if !goals.contains(&position.coord) {
                    if goals.is_empty() {
                        clear_construction_haul(world, npc);
                    } else {
                        set_route(world, npc, goals);
                    }
                    continue;
                }
                let carried = world
                    .get::<NpcInventory>(npc)
                    .map_or(0, |inv| inv.contents().get(haul.kind));
                let Some(progress) = world.get::<ConstructionProgress>(haul.blueprint).copied()
                else {
                    clear_construction_haul(world, npc);
                    continue;
                };
                let amount = carried.min(haul.amount).min(
                    progress.remaining(blueprint.kind.definition().construction_cost(), haul.kind),
                );
                if amount > 0
                    && world
                        .get_mut::<NpcInventory>(npc)
                        .is_some_and(|mut inv| inv.consume(haul.kind, amount))
                {
                    if let Some(mut progress) =
                        world.get_mut::<ConstructionProgress>(haul.blueprint)
                    {
                        progress.deposit(
                            haul.kind,
                            amount,
                            blueprint.kind.definition().construction_cost(),
                        );
                    }
                }
                clear_construction_haul(world, npc);
            }
        }
    }
}

fn food_sources(world: &World) -> Vec<StockEndpoint> {
    let mut sources = Vec::new();
    if let Some(mut warehouses) =
        world.try_query::<(Entity, &crate::buildings::WarehouseInventory)>()
    {
        sources.extend(
            warehouses
                .iter(world)
                .filter(|(_, inventory)| inventory.contents().get(ResourceKind::Food) > 0)
                .map(|(entity, _)| StockEndpoint::Warehouse(entity)),
        );
    }
    if let Some(mut refineries) = world.try_query::<(Entity, &Building, &RefineryInventory)>() {
        sources.extend(
            refineries
                .iter(world)
                .filter(|(_, building, inventory)| {
                    building.kind == crate::buildings::BuildingKind::Kitchen
                        && inventory.output_contents().get(ResourceKind::Food) > 0
                })
                .map(|(entity, _, _)| StockEndpoint::RefineryOutput(entity)),
        );
    }
    sources
}

fn choose_food_source(
    world: &World,
    snapshot: &NavigationSnapshot,
    sources: &[StockEndpoint],
    origin: crate::grid::CellCoord,
    npc: Entity,
    amount: u32,
) -> Option<AiFoodHaul> {
    if amount == 0 {
        return None;
    }
    let viable_sources = sources
        .iter()
        .copied()
        .filter_map(|source| {
            let entity = endpoint_entity(source);
            let reserved = world
                .resource::<ReservationLedger>()
                .reserved_from(source, ResourceKind::Food);
            let available =
                source_stock(world, source, ResourceKind::Food).saturating_sub(reserved);
            if available == 0 {
                return None;
            }
            Some((source, entity.to_bits(), available))
        })
        .collect::<Vec<_>>();
    if viable_sources.is_empty() {
        return None;
    }
    let distances = snapshot.distances_from(origin)?;
    viable_sources
        .into_iter()
        .filter_map(|(source, entity_bits, available)| {
            let (_, distance) = distances
                .closest_reachable(source_interaction_cells(world, snapshot, source, npc))?;
            Some((
                distance,
                entity_bits,
                AiFoodHaul {
                    source,
                    amount: amount.min(available),
                },
            ))
        })
        .min_by_key(|candidate| (candidate.0, candidate.1))
        .map(|candidate| candidate.2)
}

fn distance_to_any(
    snapshot: &NavigationSnapshot,
    origin: crate::grid::CellCoord,
    distances: &mut Option<NavigationDistances>,
    initialized: &mut bool,
    goals: impl IntoIterator<Item = crate::grid::CellCoord>,
) -> Option<usize> {
    if !*initialized {
        *distances = snapshot.distances_from(origin);
        *initialized = true;
    }
    distances
        .as_ref()?
        .closest_reachable(goals)
        .map(|(_, distance)| distance)
}

fn food_source_available(world: &World, source: StockEndpoint) -> u32 {
    match source {
        StockEndpoint::Warehouse(_) | StockEndpoint::RefineryOutput(_) => {
            source_stock(world, source, ResourceKind::Food)
        }
        _ => 0,
    }
}

fn blueprint_goals(
    snapshot: &NavigationSnapshot,
    blueprint: BuildingBlueprint,
) -> Vec<crate::grid::CellCoord> {
    if matches!(
        blueprint.kind,
        crate::buildings::BuildingKind::Field | crate::buildings::BuildingKind::TreePlot
    ) {
        snapshot.footprint_interaction_cells(blueprint.footprint)
    } else {
        snapshot.exterior_interaction_cells(blueprint.footprint)
    }
}

fn preempt_lower_priority_work(world: &mut World, npc: Entity) {
    world
        .entity_mut(npc)
        .remove::<AiGatherResource>()
        .remove::<AiConstructBuilding>()
        .remove::<AiConstructionHaul>()
        .remove::<AiSeedField>()
        .remove::<AiHarvestField>()
        .remove::<AiSeedTreePlot>()
        .remove::<AiCutTreePlot>()
        .remove::<AiIdleRoam>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>();
    // AiRefineResource remains for one system boundary so refining can transfer
    // an already-consumed batch back to refinery-owned state before releasing it.
}

fn clear_food_haul(world: &mut World, npc: Entity) {
    world
        .resource_mut::<ReservationLedger>()
        .release_worker(npc);
    world
        .entity_mut(npc)
        .remove::<AiFoodHaul>()
        .remove::<AiSearchForFood>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>();
}

fn clear_construction_haul(world: &mut World, npc: Entity) {
    world
        .resource_mut::<ReservationLedger>()
        .release_worker(npc);
    world
        .entity_mut(npc)
        .remove::<AiConstructionHaul>()
        .remove::<AiConstructBuilding>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>();
}

fn set_route(world: &mut World, npc: Entity, goals: Vec<crate::grid::CellCoord>) {
    if !world
        .get::<NpcRoute>(npc)
        .is_some_and(|route| route.goals() == goals.as_slice())
    {
        world.entity_mut(npc).insert(NpcRoute::new(goals));
        world.entity_mut(npc).remove::<MovementTarget>();
    }
}
