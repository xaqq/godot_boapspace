use bevy_ecs::prelude::*;

use crate::ai::{
    AiConstructBuilding, AiGatherResource, AiIdleRoam, AiSearchForFood,
    CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE, DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD,
    DEFAULT_NPC_FOOD_INVENTORY_TARGET, RESOURCE_GATHER_TICKS_PER_UNIT,
};
use crate::buildings::{
    Building, BuildingActivity, BuildingBlueprint, ConstructionProgress, RefineryPullConfig,
    StorageInventory, StoragePullConfig,
};
use crate::components::{
    AiKeepEnoughFoodInInventory, CarriedResource, FoodPouch, MovementTarget, Npc, NpcPosition,
    ResourceNode, TilePosition, Wheelbarrow, CARRIED_RESOURCE_CAPACITY, WHEELBARROW_CAPACITY,
};
use crate::farming::{AiHarvestField, AiSeedField, FarmInventory};
use crate::forestry::{AiCutTreePlot, AiSeedTreePlot, ForesterLodgeInventory};
use crate::navigation::{
    current_navigation_snapshot, refresh_navigation_snapshot_cells, NavigationDistances,
    NavigationSnapshot, NpcRoute,
};
use crate::refining::{
    cancel_refining_work_for_building, endpoint_entity, recipes_for_building,
    source_interaction_cells, source_stock, stock_sources, withdraw_source, RefineryInventory,
    Reservation, ReservationLedger, SinkEndpoint, StockEndpoint,
};
use crate::resources::ResourceKind;
use crate::roads::RoadBlueprint;
use crate::skills::{NpcSkills, SkillKind};
use crate::work::NpcWorkState;

const NATURAL_RESOURCE_CONSTRUCTION_BATCH_SIZE: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiFoodHaul {
    source: StockEndpoint,
    amount: u32,
    uses_wheelbarrow: bool,
}

impl AiFoodHaul {
    pub const fn source(self) -> StockEndpoint {
        self.source
    }
    pub const fn amount(self) -> u32 {
        self.amount
    }
    pub const fn uses_wheelbarrow(self) -> bool {
        self.uses_wheelbarrow
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
    uses_wheelbarrow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingHaulPhase {
    ToSource,
    Gathering { progress_ticks: u32 },
    ToSink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiBuildingHaul {
    source: StockEndpoint,
    sink: SinkEndpoint,
    kind: ResourceKind,
    amount: u32,
    phase: BuildingHaulPhase,
    uses_wheelbarrow: bool,
}

impl AiBuildingHaul {
    pub const fn source(self) -> StockEndpoint {
        self.source
    }
    pub const fn sink(self) -> SinkEndpoint {
        self.sink
    }
    pub const fn kind(self) -> ResourceKind {
        self.kind
    }
    pub const fn amount(self) -> u32 {
        self.amount
    }
    pub const fn phase(self) -> BuildingHaulPhase {
        self.phase
    }
    pub const fn uses_wheelbarrow(self) -> bool {
        self.uses_wheelbarrow
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Default)]
pub struct AiWheelbarrowRecovery {
    sink: Option<Entity>,
}

impl AiWheelbarrowRecovery {
    pub const fn sink(self) -> Option<Entity> {
        self.sink
    }
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
    pub const fn uses_wheelbarrow(self) -> bool {
        self.uses_wheelbarrow
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
        &FoodPouch,
        Option<&AiKeepEnoughFoodInInventory>,
        Option<&AiSearchForFood>,
        Option<&AiFoodHaul>,
    ), With<Npc>>();
    let mut npcs = query
        .iter(world)
        .map(|(entity, position, food_pouch, goal, search, haul)| {
            (
                entity,
                *position,
                *food_pouch,
                goal.copied(),
                search.is_some(),
                haul.copied(),
            )
        })
        .collect::<Vec<_>>();
    npcs.sort_unstable_by_key(|(entity, ..)| entity.to_bits());

    for (npc, position, food_pouch, goal, searching, haul) in npcs {
        let start = goal.map_or(DEFAULT_NPC_FOOD_INVENTORY_START_THRESHOLD, |goal| {
            goal.start_threshold()
        });
        let target = goal.map_or(DEFAULT_NPC_FOOD_INVENTORY_TARGET, |goal| goal.target());
        let carried = food_pouch.amount();
        if searching && (carried >= target || food_pouch.free_size() == 0) {
            clear_food_haul(world, npc);
            continue;
        }
        if !searching && carried > start {
            continue;
        }

        let current = haul.filter(|haul| food_source_available(world, haul.source) >= haul.amount);
        let continuing_existing_haul = current.is_some();
        let selected = current.or_else(|| {
            choose_food_source(
                world,
                &snapshot,
                &food_sources,
                position.coord,
                npc,
                target.saturating_sub(carried).min(food_pouch.free_size()),
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
        }
        if !continuing_existing_haul {
            world
                .resource_mut::<ReservationLedger>()
                .release_worker(npc);
            let _ = world
                .resource_mut::<ReservationLedger>()
                .claim(Reservation {
                    worker: npc,
                    source: Some(haul.source),
                    sink: SinkEndpoint::FoodPouch(npc),
                    kind: ResourceKind::Food,
                    amount: haul.amount,
                    task: npc,
                });
            world.entity_mut(npc).insert(haul);
            if haul.uses_wheelbarrow() && world.get::<Wheelbarrow>(npc).is_none() {
                world.entity_mut(npc).insert(Wheelbarrow::empty());
            }
        }

        let goals = source_interaction_cells(world, &snapshot, haul.source, npc);
        if goals.contains(&position.coord) {
            let can_add = world
                .get::<FoodPouch>(npc)
                .is_some_and(|inv| inv.free_size() >= haul.amount);
            if can_add
                && withdraw_source(world, haul.source, ResourceKind::Food, haul.amount)
                && world
                    .get_mut::<FoodPouch>(npc)
                    .is_some_and(|mut pouch| pouch.add(haul.amount))
            {
                if world
                    .get::<FoodPouch>(npc)
                    .is_some_and(|pouch| pouch.amount() >= target || pouch.free_size() == 0)
                {
                    clear_food_haul(world, npc);
                } else {
                    reset_food_haul_keep_searching(world, npc);
                }
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
        &CarriedResource,
        Option<&Wheelbarrow>,
        NpcWorkState,
    ), With<Npc>>();
    let mut npcs = npc_query
        .iter(world)
        .filter(|(_, _, _, wheelbarrow, work)| wheelbarrow.is_none() && !work.is_assigned())
        .map(|(entity, position, inventory, _, _)| (entity, *position, *inventory))
        .collect::<Vec<_>>();
    npcs.sort_unstable_by_key(|(entity, ..)| entity.to_bits());

    let mut blueprint_query = world.query::<(
        Entity,
        &ConstructionProgress,
        Option<&BuildingBlueprint>,
        Option<&RoadBlueprint>,
    )>();
    let blueprints = blueprint_query
        .iter(world)
        .filter_map(|(entity, progress, building, road)| {
            let cost = building
                .map(|blueprint| blueprint.kind.definition().construction_cost())
                .or_else(|| road.map(|blueprint| blueprint.target_tier.material_cost()))?;
            Some((entity, cost, *progress))
        })
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
        for (blueprint_entity, cost, progress) in &blueprints {
            for kind in ResourceKind::ALL {
                let reserved = world
                    .resource::<ReservationLedger>()
                    .reserved_to(SinkEndpoint::Blueprint(*blueprint_entity), kind);
                let remaining = progress.remaining(*cost, kind).saturating_sub(reserved);
                if remaining == 0 {
                    continue;
                }
                let amount = remaining
                    .min(crate::components::CARRIED_RESOURCE_CAPACITY)
                    .min(inventory.free_size().max(inventory.contents().get(kind)));
                if amount == 0 {
                    continue;
                }
                if inventory.contents().get(kind) > 0 {
                    let goals = construction_goals(world, &snapshot, *blueprint_entity);
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
                            false,
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
                        StockEndpoint::Warehouse(_) => WHEELBARROW_CAPACITY,
                        _ => {
                            CONSTRUCTION_RESOURCE_DEPOSIT_BATCH_SIZE.min(CARRIED_RESOURCE_CAPACITY)
                        }
                    };
                    let uses_wheelbarrow = matches!(source, StockEndpoint::Warehouse(_));
                    candidates.push((
                        distance,
                        blueprint_entity.to_bits(),
                        kind as usize,
                        *blueprint_entity,
                        kind,
                        Some(source),
                        remaining.min(available).min(source_batch_size),
                        uses_wheelbarrow,
                    ));
                }
            }
        }
        let Some((_, _, _, blueprint, kind, source, amount, uses_wheelbarrow)) =
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
        if uses_wheelbarrow {
            world.entity_mut(npc).insert(Wheelbarrow::empty());
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
                uses_wheelbarrow,
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
        let Some(cost) = construction_cost(world, haul.blueprint) else {
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
                if !goals.contains(&position.coord) && source != StockEndpoint::CarriedResource(npc)
                {
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
                } else if withdraw_source(world, source, haul.kind, haul.amount)
                    && construction_cargo_add(world, npc, haul, haul.amount)
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
                    .get_mut::<CarriedResource>(npc)
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
                let goals = construction_goals(world, snapshot, haul.blueprint);
                if !goals.contains(&position.coord) {
                    if goals.is_empty() {
                        clear_construction_haul(world, npc);
                    } else {
                        set_route(world, npc, goals);
                    }
                    continue;
                }
                let carried = construction_cargo_amount(world, npc, haul);
                let Some(progress) = world.get::<ConstructionProgress>(haul.blueprint).copied()
                else {
                    clear_construction_haul(world, npc);
                    continue;
                };
                let amount = carried
                    .min(haul.amount)
                    .min(progress.remaining(cost, haul.kind));
                if amount > 0 && construction_cargo_consume(world, npc, haul, amount) {
                    if let Some(mut progress) =
                        world.get_mut::<ConstructionProgress>(haul.blueprint)
                    {
                        progress.deposit(haul.kind, amount, cost);
                    }
                }
                clear_construction_haul(world, npc);
            }
        }
    }
}

fn construction_cargo_add(
    world: &mut World,
    worker: Entity,
    haul: AiConstructionHaul,
    amount: u32,
) -> bool {
    if haul.uses_wheelbarrow() {
        world
            .get_mut::<Wheelbarrow>(worker)
            .is_some_and(|mut cargo| cargo.add(haul.kind(), amount))
    } else {
        world
            .get_mut::<CarriedResource>(worker)
            .is_some_and(|mut cargo| cargo.add(haul.kind(), amount))
    }
}

fn construction_cargo_amount(world: &World, worker: Entity, haul: AiConstructionHaul) -> u32 {
    if haul.uses_wheelbarrow() {
        world
            .get::<Wheelbarrow>(worker)
            .map_or(0, |cargo| cargo.contents().get(haul.kind()))
    } else {
        world
            .get::<CarriedResource>(worker)
            .map_or(0, |cargo| cargo.contents().get(haul.kind()))
    }
}

fn construction_cargo_consume(
    world: &mut World,
    worker: Entity,
    haul: AiConstructionHaul,
    amount: u32,
) -> bool {
    if haul.uses_wheelbarrow() {
        world
            .get_mut::<Wheelbarrow>(worker)
            .is_some_and(|mut cargo| cargo.consume(haul.kind(), amount))
    } else {
        world
            .get_mut::<CarriedResource>(worker)
            .is_some_and(|mut cargo| cargo.consume(haul.kind(), amount))
    }
}

#[derive(Debug, Clone, Copy)]
struct BuildingHaulCandidate {
    distance: usize,
    source_bits: u64,
    sink_bits: u64,
    kind: ResourceKind,
    source: StockEndpoint,
    sink: SinkEndpoint,
    amount: u32,
    uses_wheelbarrow: bool,
}

/// Advances and assigns unskilled refinery-supply and refinery-output storage
/// pulls. Every claim reserves both its source stock and destination capacity.
pub fn manage_building_logistics(world: &mut World) {
    if world.get_resource::<ReservationLedger>().is_none() {
        world.insert_resource(ReservationLedger::default());
    }
    let Some(snapshot) = current_navigation_snapshot(world) else {
        return;
    };
    advance_building_hauls(world, &snapshot);

    let mut query = world.query_filtered::<(
        Entity,
        &NpcPosition,
        &CarriedResource,
        Option<&Wheelbarrow>,
        NpcWorkState,
    ), With<Npc>>();
    let mut workers = query
        .iter(world)
        .filter(|(_, _, cargo, wheelbarrow, work)| {
            cargo.used_size() == 0 && wheelbarrow.is_none() && !work.is_assigned()
        })
        .map(|(entity, position, _, _, _)| (entity, *position))
        .collect::<Vec<_>>();
    workers.sort_unstable_by_key(|(entity, _)| entity.to_bits());

    for (worker, position) in workers {
        let Some(distances) = snapshot.distances_from(position.coord) else {
            continue;
        };
        let mut candidates = storage_pull_candidates(world, &snapshot, &distances);
        candidates.extend(refinery_supply_candidates(world, &snapshot, &distances));
        let Some(candidate) = candidates.into_iter().min_by_key(|candidate| {
            (
                candidate.distance,
                candidate.source_bits,
                candidate.sink_bits,
                candidate.kind as usize,
            )
        }) else {
            continue;
        };
        let reservation = Reservation {
            worker,
            source: Some(candidate.source),
            sink: candidate.sink,
            kind: candidate.kind,
            amount: candidate.amount,
            task: sink_entity(candidate.sink),
        };
        if !world.resource_mut::<ReservationLedger>().claim(reservation) {
            continue;
        }
        if candidate.uses_wheelbarrow {
            world.entity_mut(worker).insert(Wheelbarrow::empty());
        }
        world.entity_mut(worker).insert(AiBuildingHaul {
            source: candidate.source,
            sink: candidate.sink,
            kind: candidate.kind,
            amount: candidate.amount,
            phase: BuildingHaulPhase::ToSource,
            uses_wheelbarrow: candidate.uses_wheelbarrow,
        });
    }
}

fn storage_pull_candidates(
    world: &mut World,
    snapshot: &NavigationSnapshot,
    distances: &NavigationDistances,
) -> Vec<BuildingHaulCandidate> {
    let mut storages = world
        .query::<(Entity, &Building, &StorageInventory, &StoragePullConfig)>()
        .iter(world)
        .filter(|(entity, building, _, _)| {
            building.kind.is_storage() && building_is_active(world, *entity)
        })
        .map(|(entity, building, inventory, pull)| (entity, *building, *inventory, *pull))
        .collect::<Vec<_>>();
    storages.sort_unstable_by_key(|(entity, ..)| entity.to_bits());
    let mut refineries = world
        .query::<(Entity, &Building, &RefineryInventory)>()
        .iter(world)
        .filter(|(entity, _, _)| building_is_active(world, *entity))
        .map(|(entity, building, inventory)| (entity, *building, *inventory))
        .collect::<Vec<_>>();
    refineries.sort_unstable_by_key(|(entity, ..)| entity.to_bits());

    let mut candidates = Vec::new();
    for (storage, _, inventory, pull) in storages {
        for kind in StoragePullConfig::SUPPORTED_RESOURCES {
            if !pull.pulls_from_refineries(kind) || !inventory.is_allowed(kind) {
                continue;
            }
            let reserved_to = world
                .resource::<ReservationLedger>()
                .reserved_capacity_to(SinkEndpoint::Storage(storage));
            let free = inventory.free_size().saturating_sub(reserved_to);
            if free == 0 {
                continue;
            }
            for (source_entity, _, source_inventory) in &refineries {
                let source = StockEndpoint::RefineryOutput(*source_entity);
                let available = source_inventory.output_contents().get(kind).saturating_sub(
                    world
                        .resource::<ReservationLedger>()
                        .reserved_from(source, kind),
                );
                if available == 0 {
                    continue;
                }
                let goals = source_interaction_cells(world, snapshot, source, Entity::PLACEHOLDER);
                let Some((_, distance)) = distances.closest_reachable(goals) else {
                    continue;
                };
                candidates.push(BuildingHaulCandidate {
                    distance,
                    source_bits: source_entity.to_bits(),
                    sink_bits: storage.to_bits(),
                    kind,
                    source,
                    sink: SinkEndpoint::Storage(storage),
                    amount: available.min(free).min(WHEELBARROW_CAPACITY),
                    uses_wheelbarrow: true,
                });
            }
        }
    }
    candidates
}

fn refinery_supply_candidates(
    world: &mut World,
    snapshot: &NavigationSnapshot,
    distances: &NavigationDistances,
) -> Vec<BuildingHaulCandidate> {
    let mut refineries = world
        .query::<(Entity, &Building, &RefineryInventory, &RefineryPullConfig)>()
        .iter(world)
        .filter(|(entity, _, _, _)| building_is_active(world, *entity))
        .map(|(entity, building, inventory, pull)| (entity, *building, *inventory, *pull))
        .collect::<Vec<_>>();
    refineries.sort_unstable_by_key(|(entity, ..)| entity.to_bits());
    let mut candidates = Vec::new();
    for (refinery, building, inventory, pull) in refineries {
        for recipe in recipes_for_building(building.kind) {
            let kind = recipe.definition().input();
            let reserved_to = world
                .resource::<ReservationLedger>()
                .reserved_capacity_to(SinkEndpoint::RefineryInput(refinery));
            let free = inventory.input_free_size().saturating_sub(reserved_to);
            if free == 0 {
                continue;
            }
            let storage_only = pull.pulls_from_storage(kind);
            for source in supply_sources(world, kind, storage_only) {
                let reserved_from = world
                    .resource::<ReservationLedger>()
                    .reserved_from(source, kind);
                let available = source_stock(world, source, kind).saturating_sub(reserved_from);
                if available == 0 {
                    continue;
                }
                let goals = source_interaction_cells(world, snapshot, source, Entity::PLACEHOLDER);
                let Some((_, distance)) = distances.closest_reachable(goals) else {
                    continue;
                };
                let uses_wheelbarrow = matches!(source, StockEndpoint::Warehouse(_));
                let capacity = if uses_wheelbarrow {
                    WHEELBARROW_CAPACITY
                } else if matches!(source, StockEndpoint::NaturalNode(_)) {
                    1
                } else {
                    CARRIED_RESOURCE_CAPACITY
                };
                candidates.push(BuildingHaulCandidate {
                    distance,
                    source_bits: endpoint_entity(source).to_bits(),
                    sink_bits: refinery.to_bits(),
                    kind,
                    source,
                    sink: SinkEndpoint::RefineryInput(refinery),
                    amount: available.min(free).min(capacity),
                    uses_wheelbarrow,
                });
            }
        }
    }
    candidates
}

fn supply_sources(world: &mut World, kind: ResourceKind, storage_only: bool) -> Vec<StockEndpoint> {
    let mut sources = Vec::new();
    if storage_only {
        if let Some(mut query) = world.try_query::<(Entity, &Building, &StorageInventory)>() {
            sources.extend(
                query
                    .iter(world)
                    .filter_map(|(entity, building, inventory)| {
                        (building.kind.is_storage()
                            && building_is_active(world, entity)
                            && inventory.contents().get(kind) > 0)
                            .then_some(StockEndpoint::Warehouse(entity))
                    }),
            );
        }
    } else {
        if let Some(mut query) = world.try_query::<(Entity, &ResourceNode)>() {
            sources.extend(query.iter(world).filter_map(|(entity, node)| {
                (node.kind == kind && node.quantity > 0)
                    .then_some(StockEndpoint::NaturalNode(entity))
            }));
        }
        if kind == ResourceKind::Crops {
            if let Some(mut query) = world.try_query::<(Entity, &FarmInventory)>() {
                sources.extend(query.iter(world).filter_map(|(entity, inventory)| {
                    (inventory.contents().get(kind) > 0).then_some(StockEndpoint::Farm(entity))
                }));
            }
        }
        if kind == ResourceKind::Wood {
            if let Some(mut query) = world.try_query::<(Entity, &ForesterLodgeInventory)>() {
                sources.extend(query.iter(world).filter_map(|(entity, inventory)| {
                    (inventory.contents().get(kind) > 0)
                        .then_some(StockEndpoint::ForesterLodge(entity))
                }));
            }
        }
    }
    sources.sort_unstable_by_key(|source| endpoint_entity(*source).to_bits());
    sources
}

fn advance_building_hauls(world: &mut World, snapshot: &NavigationSnapshot) {
    let mut query = world.query::<(Entity, &NpcPosition, &AiBuildingHaul)>();
    let mut hauls = query
        .iter(world)
        .map(|(worker, position, haul)| (worker, *position, *haul))
        .collect::<Vec<_>>();
    hauls.sort_unstable_by_key(|(worker, ..)| worker.to_bits());
    for (worker, position, mut haul) in hauls {
        match haul.phase {
            BuildingHaulPhase::ToSource => {
                if !source_is_active(world, haul.source)
                    || source_stock(world, haul.source, haul.kind) < haul.amount
                {
                    clear_building_haul(world, worker, true);
                    continue;
                }
                let goals = source_interaction_cells(world, snapshot, haul.source, worker);
                if goals.is_empty() {
                    clear_building_haul(world, worker, true);
                } else if goals.contains(&position.coord) {
                    world.entity_mut(worker).remove::<NpcRoute>();
                    world.entity_mut(worker).remove::<MovementTarget>();
                    if matches!(haul.source, StockEndpoint::NaturalNode(_)) {
                        haul.phase = BuildingHaulPhase::Gathering { progress_ticks: 0 };
                        world.entity_mut(worker).insert(haul);
                    } else if load_haul(world, worker, haul) {
                        haul.phase = BuildingHaulPhase::ToSink;
                        world.entity_mut(worker).insert(haul);
                    } else {
                        clear_building_haul(world, worker, true);
                    }
                } else {
                    set_route(world, worker, goals);
                }
            }
            BuildingHaulPhase::Gathering { progress_ticks } => {
                let Some(StockEndpoint::NaturalNode(node)) = Some(haul.source) else {
                    clear_building_haul(world, worker, true);
                    continue;
                };
                let valid = world.get::<TilePosition>(node).is_some_and(|tile| {
                    snapshot
                        .point_interaction_cells(tile.coord)
                        .contains(&position.coord)
                }) && world.get::<ResourceNode>(node).is_some_and(|resource| {
                    resource.kind == haul.kind && resource.quantity >= haul.amount
                });
                if !valid {
                    clear_building_haul(world, worker, true);
                    continue;
                }
                let next = progress_ticks.saturating_add(1);
                if next < RESOURCE_GATHER_TICKS_PER_UNIT {
                    haul.phase = BuildingHaulPhase::Gathering {
                        progress_ticks: next,
                    };
                    world.entity_mut(worker).insert(haul);
                    continue;
                }
                if !cargo_add(world, worker, haul, haul.amount) {
                    clear_building_haul(world, worker, true);
                    continue;
                }
                let depleted = if let Some(mut resource) = world.get_mut::<ResourceNode>(node) {
                    resource.quantity = resource.quantity.saturating_sub(haul.amount);
                    resource.quantity == 0
                } else {
                    false
                };
                if depleted {
                    let coord = world.get::<TilePosition>(node).map(|tile| tile.coord);
                    world.entity_mut(node).remove::<ResourceNode>();
                    if let Some(coord) = coord {
                        refresh_navigation_snapshot_cells(world, [coord]);
                    }
                }
                if let Some(mut skills) = world.get_mut::<NpcSkills>(worker) {
                    if let Some(skill) = SkillKind::try_for_gathered_resource(haul.kind) {
                        skills.add_xp(skill, haul.amount);
                    }
                }
                haul.phase = BuildingHaulPhase::ToSink;
                world.entity_mut(worker).insert(haul);
            }
            BuildingHaulPhase::ToSink => {
                if !sink_can_accept(world, haul.sink, haul.kind, haul.amount) {
                    if haul.uses_wheelbarrow && wheelbarrow_loaded(world, worker) {
                        begin_wheelbarrow_recovery(world, worker);
                    } else {
                        clear_building_haul(world, worker, false);
                    }
                    continue;
                }
                let goals = sink_interaction_cells(world, snapshot, haul.sink);
                if goals.is_empty() {
                    if haul.uses_wheelbarrow && wheelbarrow_loaded(world, worker) {
                        begin_wheelbarrow_recovery(world, worker);
                    } else {
                        clear_building_haul(world, worker, false);
                    }
                } else if goals.contains(&position.coord) {
                    if deliver_haul(world, worker, haul) {
                        clear_building_haul(world, worker, true);
                    } else if haul.uses_wheelbarrow && wheelbarrow_loaded(world, worker) {
                        begin_wheelbarrow_recovery(world, worker);
                    } else {
                        clear_building_haul(world, worker, false);
                    }
                } else {
                    set_route(world, worker, goals);
                }
            }
        }
    }
}

fn load_haul(world: &mut World, worker: Entity, haul: AiBuildingHaul) -> bool {
    if !withdraw_source(world, haul.source, haul.kind, haul.amount) {
        return false;
    }
    if cargo_add(world, worker, haul, haul.amount) {
        true
    } else {
        let _ = restore_source(world, haul.source, haul.kind, haul.amount);
        false
    }
}

fn cargo_add(world: &mut World, worker: Entity, haul: AiBuildingHaul, amount: u32) -> bool {
    if haul.uses_wheelbarrow {
        world
            .get_mut::<Wheelbarrow>(worker)
            .is_some_and(|mut cargo| cargo.add(haul.kind, amount))
    } else {
        world
            .get_mut::<CarriedResource>(worker)
            .is_some_and(|mut cargo| cargo.add(haul.kind, amount))
    }
}

fn deliver_haul(world: &mut World, worker: Entity, haul: AiBuildingHaul) -> bool {
    let removed = if haul.uses_wheelbarrow {
        world
            .get_mut::<Wheelbarrow>(worker)
            .is_some_and(|mut cargo| cargo.consume(haul.kind, haul.amount))
    } else {
        world
            .get_mut::<CarriedResource>(worker)
            .is_some_and(|mut cargo| cargo.consume(haul.kind, haul.amount))
    };
    if !removed {
        return false;
    }
    if deposit_sink(world, haul.sink, haul.kind, haul.amount) {
        true
    } else {
        let _ = cargo_add(world, worker, haul, haul.amount);
        false
    }
}

fn restore_source(
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

fn deposit_sink(world: &mut World, sink: SinkEndpoint, kind: ResourceKind, amount: u32) -> bool {
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

fn sink_can_accept(world: &World, sink: SinkEndpoint, kind: ResourceKind, amount: u32) -> bool {
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

fn sink_interaction_cells(
    world: &World,
    snapshot: &NavigationSnapshot,
    sink: SinkEndpoint,
) -> Vec<crate::grid::CellCoord> {
    world
        .get::<Building>(sink_entity(sink))
        .map(|building| snapshot.exterior_interaction_cells(building.footprint))
        .unwrap_or_default()
}

fn sink_entity(sink: SinkEndpoint) -> Entity {
    match sink {
        SinkEndpoint::Blueprint(entity)
        | SinkEndpoint::FoodPouch(entity)
        | SinkEndpoint::Storage(entity)
        | SinkEndpoint::RefineryInput(entity)
        | SinkEndpoint::RefineryOutput(entity) => entity,
    }
}

fn building_is_active(world: &World, entity: Entity) -> bool {
    world
        .get::<BuildingActivity>(entity)
        .map_or(true, |activity| activity.is_active())
}

fn source_is_active(world: &World, source: StockEndpoint) -> bool {
    match source {
        StockEndpoint::Warehouse(entity)
        | StockEndpoint::RefineryInput(entity)
        | StockEndpoint::RefineryOutput(entity) => building_is_active(world, entity),
        _ => true,
    }
}

fn wheelbarrow_loaded(world: &World, worker: Entity) -> bool {
    world
        .get::<Wheelbarrow>(worker)
        .is_some_and(|wheelbarrow| wheelbarrow.stack().is_some())
}

fn clear_building_haul(world: &mut World, worker: Entity, remove_wheelbarrow: bool) {
    world
        .resource_mut::<ReservationLedger>()
        .release_worker(worker);
    let mut entity = world.entity_mut(worker);
    entity
        .remove::<AiBuildingHaul>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>();
    if remove_wheelbarrow {
        entity.remove::<Wheelbarrow>();
    }
}

fn begin_wheelbarrow_recovery(world: &mut World, worker: Entity) {
    world
        .resource_mut::<ReservationLedger>()
        .release_worker(worker);
    world
        .entity_mut(worker)
        .remove::<AiBuildingHaul>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>()
        .insert(AiWheelbarrowRecovery::default());
}

/// Immediately releases every assignment that uses the building as a source,
/// sink, or refinery. Loaded wheelbarrows retain their cargo and recover;
/// ordinary NPC cargo remains on the worker after its assignment is cleared.
pub fn cancel_work_involving_building(world: &mut World, building: Entity) {
    let building_hauls = world
        .query::<(Entity, &AiBuildingHaul)>()
        .iter(world)
        .filter(|(_, haul)| {
            endpoint_entity(haul.source()) == building || sink_entity(haul.sink()) == building
        })
        .map(|(worker, haul)| (worker, *haul))
        .collect::<Vec<_>>();
    for (worker, haul) in building_hauls {
        if haul.uses_wheelbarrow() && wheelbarrow_loaded(world, worker) {
            begin_wheelbarrow_recovery(world, worker);
        } else {
            clear_building_haul(world, worker, haul.uses_wheelbarrow());
        }
    }

    let construction = world
        .query::<(Entity, &AiConstructionHaul)>()
        .iter(world)
        .filter(|(_, haul)| {
            haul.source()
                .is_some_and(|source| endpoint_entity(source) == building)
        })
        .map(|(worker, _)| worker)
        .collect::<Vec<_>>();
    for worker in construction {
        clear_construction_haul(world, worker);
    }

    let food = world
        .query::<(Entity, &AiFoodHaul)>()
        .iter(world)
        .filter(|(_, haul)| endpoint_entity(haul.source()) == building)
        .map(|(worker, _)| worker)
        .collect::<Vec<_>>();
    for worker in food {
        clear_food_haul(world, worker);
    }

    let recoveries = world
        .query::<(Entity, &AiWheelbarrowRecovery)>()
        .iter(world)
        .filter(|(_, recovery)| recovery.sink() == Some(building))
        .map(|(worker, _)| worker)
        .collect::<Vec<_>>();
    for worker in recoveries {
        world
            .resource_mut::<ReservationLedger>()
            .release_worker(worker);
        world
            .entity_mut(worker)
            .insert(AiWheelbarrowRecovery::default());
        world.entity_mut(worker).remove::<NpcRoute>();
        world.entity_mut(worker).remove::<MovementTarget>();
    }
    cancel_refining_work_for_building(world, building);
}

/// Routes loaded, canceled wheelbarrows to the nearest valid storage. Loads
/// remain attached and are retried when no destination is currently viable.
pub fn manage_wheelbarrow_recovery(world: &mut World) {
    if world.get_resource::<ReservationLedger>().is_none() {
        world.insert_resource(ReservationLedger::default());
    }
    let Some(snapshot) = current_navigation_snapshot(world) else {
        return;
    };
    let mut query = world.query::<(Entity, &NpcPosition, &Wheelbarrow, &AiWheelbarrowRecovery)>();
    let mut recoveries = query
        .iter(world)
        .map(|(worker, position, wheelbarrow, recovery)| {
            (worker, *position, *wheelbarrow, *recovery)
        })
        .collect::<Vec<_>>();
    recoveries.sort_unstable_by_key(|(worker, ..)| worker.to_bits());
    for (worker, position, wheelbarrow, mut recovery) in recoveries {
        let Some(stack) = wheelbarrow.stack() else {
            world
                .resource_mut::<ReservationLedger>()
                .release_worker(worker);
            world
                .entity_mut(worker)
                .remove::<AiWheelbarrowRecovery>()
                .remove::<Wheelbarrow>();
            continue;
        };
        let kind = stack.kind();
        let amount = stack.amount();
        if recovery
            .sink
            .is_some_and(|sink| !sink_can_accept(world, SinkEndpoint::Storage(sink), kind, amount))
        {
            world
                .resource_mut::<ReservationLedger>()
                .release_worker(worker);
            recovery.sink = None;
            world.entity_mut(worker).insert(recovery);
        }
        if recovery.sink.is_none() {
            let Some(distances) = snapshot.distances_from(position.coord) else {
                continue;
            };
            let mut candidates = world
                .query::<(Entity, &Building, &StorageInventory)>()
                .iter(world)
                .filter_map(|(entity, building, inventory)| {
                    if !building.kind.is_storage()
                        || !building_is_active(world, entity)
                        || !inventory.is_allowed(kind)
                    {
                        return None;
                    }
                    let reserved = world
                        .resource::<ReservationLedger>()
                        .reserved_capacity_to(SinkEndpoint::Storage(entity));
                    if inventory.free_size().saturating_sub(reserved) < amount {
                        return None;
                    }
                    let goals = snapshot.exterior_interaction_cells(building.footprint);
                    let (_, distance) = distances.closest_reachable(goals)?;
                    Some((distance, entity.to_bits(), entity))
                })
                .collect::<Vec<_>>();
            candidates.sort_unstable();
            let Some((_, _, sink)) = candidates.first().copied() else {
                continue;
            };
            if !world
                .resource_mut::<ReservationLedger>()
                .claim(Reservation {
                    worker,
                    source: None,
                    sink: SinkEndpoint::Storage(sink),
                    kind,
                    amount,
                    task: sink,
                })
            {
                continue;
            }
            recovery.sink = Some(sink);
            world.entity_mut(worker).insert(recovery);
        }
        let Some(sink) = recovery.sink else {
            continue;
        };
        let goals = sink_interaction_cells(world, &snapshot, SinkEndpoint::Storage(sink));
        if goals.contains(&position.coord) {
            if deposit_sink(world, SinkEndpoint::Storage(sink), kind, amount)
                && world
                    .get_mut::<Wheelbarrow>(worker)
                    .is_some_and(|mut cargo| cargo.consume(kind, amount))
            {
                world
                    .resource_mut::<ReservationLedger>()
                    .release_worker(worker);
                world
                    .entity_mut(worker)
                    .remove::<AiWheelbarrowRecovery>()
                    .remove::<Wheelbarrow>()
                    .remove::<NpcRoute>()
                    .remove::<MovementTarget>();
            }
        } else if !goals.is_empty() {
            set_route(world, worker, goals);
        }
    }
}

fn food_sources(world: &World) -> Vec<StockEndpoint> {
    let mut sources = Vec::new();
    if let Some(mut warehouses) =
        world.try_query::<(Entity, &Building, &crate::buildings::WarehouseInventory)>()
    {
        sources.extend(
            warehouses
                .iter(world)
                .filter(|(entity, building, inventory)| {
                    building.kind.is_storage()
                        && building_is_active(world, *entity)
                        && inventory.contents().get(ResourceKind::Food) > 0
                })
                .map(|(entity, _, _)| StockEndpoint::Warehouse(entity)),
        );
    }
    if let Some(mut refineries) = world.try_query::<(Entity, &Building, &RefineryInventory)>() {
        sources.extend(
            refineries
                .iter(world)
                .filter(|(entity, building, inventory)| {
                    building.kind == crate::buildings::BuildingKind::Kitchen
                        && building_is_active(world, *entity)
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
                    uses_wheelbarrow: matches!(source, StockEndpoint::Warehouse(_)),
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

fn construction_cost(world: &World, entity: Entity) -> Option<crate::resources::ResourceAmounts> {
    world
        .get::<BuildingBlueprint>(entity)
        .map(|blueprint| blueprint.kind.definition().construction_cost())
        .or_else(|| {
            world
                .get::<RoadBlueprint>(entity)
                .map(|blueprint| blueprint.target_tier.material_cost())
        })
}

fn construction_goals(
    world: &World,
    snapshot: &NavigationSnapshot,
    entity: Entity,
) -> Vec<crate::grid::CellCoord> {
    if let Some(blueprint) = world.get::<BuildingBlueprint>(entity).copied() {
        return blueprint_goals(snapshot, blueprint);
    }
    world
        .get::<RoadBlueprint>(entity)
        .filter(|blueprint| snapshot.is_walkable(blueprint.coord))
        .map(|blueprint| vec![blueprint.coord])
        .unwrap_or_default()
}

fn preempt_lower_priority_work(world: &mut World, npc: Entity) {
    if wheelbarrow_loaded(world, npc)
        && (world.get::<AiBuildingHaul>(npc).is_some()
            || world.get::<AiConstructionHaul>(npc).is_some())
    {
        begin_wheelbarrow_recovery(world, npc);
    } else if world.get::<Wheelbarrow>(npc).is_some()
        && world.get::<AiWheelbarrowRecovery>(npc).is_none()
    {
        world.entity_mut(npc).remove::<Wheelbarrow>();
    }
    world
        .entity_mut(npc)
        .remove::<AiGatherResource>()
        .remove::<AiConstructBuilding>()
        .remove::<AiConstructionHaul>()
        .remove::<AiBuildingHaul>()
        .remove::<crate::tasks::AiConstructionLabor>()
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
    let remove_wheelbarrow = world
        .get::<AiFoodHaul>(npc)
        .is_some_and(|haul| haul.uses_wheelbarrow())
        && world.get::<AiWheelbarrowRecovery>(npc).is_none();
    world
        .resource_mut::<ReservationLedger>()
        .release_worker(npc);
    world
        .entity_mut(npc)
        .remove::<AiFoodHaul>()
        .remove::<AiSearchForFood>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>();
    if remove_wheelbarrow {
        world.entity_mut(npc).remove::<Wheelbarrow>();
    }
}

fn reset_food_haul_keep_searching(world: &mut World, npc: Entity) {
    let remove_wheelbarrow = world
        .get::<AiFoodHaul>(npc)
        .is_some_and(|haul| haul.uses_wheelbarrow())
        && world.get::<AiWheelbarrowRecovery>(npc).is_none();
    world
        .resource_mut::<ReservationLedger>()
        .release_worker(npc);
    world
        .entity_mut(npc)
        .remove::<AiFoodHaul>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>();
    if remove_wheelbarrow {
        world.entity_mut(npc).remove::<Wheelbarrow>();
    }
}

fn clear_construction_haul(world: &mut World, npc: Entity) {
    world
        .resource_mut::<ReservationLedger>()
        .release_worker(npc);
    let loaded_wheelbarrow = wheelbarrow_loaded(world, npc);
    let mut entity = world.entity_mut(npc);
    entity
        .remove::<AiConstructionHaul>()
        .remove::<AiConstructBuilding>()
        .remove::<NpcRoute>()
        .remove::<MovementTarget>();
    if loaded_wheelbarrow {
        entity.insert(AiWheelbarrowRecovery::default());
    } else {
        entity.remove::<Wheelbarrow>();
    }
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
