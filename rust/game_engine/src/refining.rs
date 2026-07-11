use std::collections::{HashMap, HashSet};

use bevy_ecs::prelude::*;

use crate::ai::RESOURCE_GATHER_TICKS_PER_UNIT;
use crate::buildings::{Building, BuildingKind, WarehouseInventory};
use crate::components::{
    AiConstructBuilding, AiGatherResource, AiSearchForFood, MovementTarget, Npc, NpcInventory,
    NpcPosition, ResourceNode, TilePosition,
};
use crate::farming::{AiHarvestField, AiSeedField, FarmInventory};
use crate::forestry::{AiCutTreePlot, AiSeedTreePlot, ForesterLodgeInventory};
use crate::navigation::{
    current_navigation_snapshot, refresh_navigation_snapshot_cells, NavigationDistances,
    NavigationSnapshot, NpcRoute,
};
use crate::resources::{ResourceAmounts, ResourceInventory, ResourceKind};
use crate::skills::{Cook, NpcSkills, Sawyer, SkillKind, Stonemason};
use crate::tasks::{Task, TaskAssignment};

pub const REFINERY_INPUT_CAPACITY: u32 = 100;
pub const REFINERY_OUTPUT_CAPACITY: u32 = 100;
pub const REFINING_TICKS_PER_UNIT: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecipeKind {
    SawWood,
    CutStone,
    CookCrops,
    CookWildBerries,
}

impl RecipeKind {
    pub const ALL: [Self; 4] = [
        Self::SawWood,
        Self::CutStone,
        Self::CookCrops,
        Self::CookWildBerries,
    ];

    pub const fn definition(self) -> RecipeDefinition {
        match self {
            Self::SawWood => RecipeDefinition::new(
                BuildingKind::Sawmill,
                ResourceKind::Wood,
                ResourceKind::Planks,
                SkillKind::Sawyer,
            ),
            Self::CutStone => RecipeDefinition::new(
                BuildingKind::Stoneworks,
                ResourceKind::Stone,
                ResourceKind::StoneBlocks,
                SkillKind::Stonemason,
            ),
            Self::CookCrops => RecipeDefinition::new(
                BuildingKind::Kitchen,
                ResourceKind::Crops,
                ResourceKind::Food,
                SkillKind::Cook,
            ),
            Self::CookWildBerries => RecipeDefinition::new(
                BuildingKind::Kitchen,
                ResourceKind::WildBerries,
                ResourceKind::Food,
                SkillKind::Cook,
            ),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SawWood => "Wood → Plank",
            Self::CutStone => "Stone → Stone Block",
            Self::CookCrops => "Crops → Food",
            Self::CookWildBerries => "Wild Berries → Food",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeDefinition {
    building: BuildingKind,
    input: ResourceKind,
    output: ResourceKind,
    skill: SkillKind,
}

impl RecipeDefinition {
    const fn new(
        building: BuildingKind,
        input: ResourceKind,
        output: ResourceKind,
        skill: SkillKind,
    ) -> Self {
        Self {
            building,
            input,
            output,
            skill,
        }
    }

    pub const fn building(self) -> BuildingKind {
        self.building
    }
    pub const fn input(self) -> ResourceKind {
        self.input
    }
    pub const fn output(self) -> ResourceKind {
        self.output
    }
    pub const fn duration_ticks(self) -> u32 {
        REFINING_TICKS_PER_UNIT
    }
    pub const fn skill(self) -> SkillKind {
        self.skill
    }
}

pub const fn recipes_for_building(kind: BuildingKind) -> &'static [RecipeKind] {
    match kind {
        BuildingKind::Sawmill => &[RecipeKind::SawWood],
        BuildingKind::Stoneworks => &[RecipeKind::CutStone],
        BuildingKind::Kitchen => &[RecipeKind::CookCrops, RecipeKind::CookWildBerries],
        _ => &[],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct RefineryInventory {
    input: ResourceInventory,
    output: ResourceInventory,
}

impl RefineryInventory {
    pub const fn empty() -> Self {
        Self {
            input: ResourceInventory::empty(REFINERY_INPUT_CAPACITY),
            output: ResourceInventory::empty(REFINERY_OUTPUT_CAPACITY),
        }
    }

    pub const fn input_contents(self) -> ResourceAmounts {
        self.input.contents()
    }
    pub const fn output_contents(self) -> ResourceAmounts {
        self.output.contents()
    }
    pub const fn input_capacity(self) -> u32 {
        self.input.max_size()
    }
    pub const fn output_capacity(self) -> u32 {
        self.output.max_size()
    }
    pub const fn input_free_size(self) -> u32 {
        self.input.free_size()
    }
    pub const fn output_free_size(self) -> u32 {
        self.output.free_size()
    }
    pub fn add_input(&mut self, building: BuildingKind, kind: ResourceKind, amount: u32) -> bool {
        recipes_for_building(building)
            .iter()
            .any(|recipe| recipe.definition().input() == kind)
            && self.input.add(kind, amount)
    }
    pub fn consume_input(&mut self, kind: ResourceKind, amount: u32) -> bool {
        self.input.consume(kind, amount)
    }
    pub fn add_output(&mut self, building: BuildingKind, kind: ResourceKind, amount: u32) -> bool {
        recipes_for_building(building)
            .iter()
            .any(|recipe| recipe.definition().output() == kind)
            && self.output.add(kind, amount)
    }
    pub fn consume_output(&mut self, kind: ResourceKind, amount: u32) -> bool {
        self.output.consume(kind, amount)
    }
}

impl Default for RefineryInventory {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Default)]
pub struct RefineryProduction {
    recipe: Option<RecipeKind>,
    progress_ticks: u32,
    assigned_worker: Option<Entity>,
    output_reserved: bool,
}

impl RefineryProduction {
    pub const fn recipe(self) -> Option<RecipeKind> {
        self.recipe
    }
    pub const fn progress_ticks(self) -> u32 {
        self.progress_ticks
    }
    pub const fn remaining_ticks(self) -> u32 {
        REFINING_TICKS_PER_UNIT.saturating_sub(self.progress_ticks)
    }
    pub const fn assigned_worker(self) -> Option<Entity> {
        self.assigned_worker
    }
    pub const fn is_active(self) -> bool {
        self.recipe.is_some() && self.progress_ticks < REFINING_TICKS_PER_UNIT
    }
    pub const fn is_awaiting_output(self) -> bool {
        self.recipe.is_some() && self.progress_ticks >= REFINING_TICKS_PER_UNIT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StockEndpoint {
    NaturalNode(Entity),
    NpcInventory(Entity),
    Warehouse(Entity),
    Farm(Entity),
    ForesterLodge(Entity),
    RefineryInput(Entity),
    RefineryOutput(Entity),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SinkEndpoint {
    Blueprint(Entity),
    NpcInventory(Entity),
    RefineryInput(Entity),
    RefineryOutput(Entity),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reservation {
    pub worker: Entity,
    pub source: Option<StockEndpoint>,
    pub sink: SinkEndpoint,
    pub kind: ResourceKind,
    pub amount: u32,
    pub task: Entity,
}

#[derive(Debug, Default, Resource)]
pub struct ReservationLedger {
    claims: Vec<Reservation>,
}

impl ReservationLedger {
    pub fn claims(&self) -> &[Reservation] {
        &self.claims
    }
    pub fn reserved_from(&self, source: StockEndpoint, kind: ResourceKind) -> u32 {
        self.claims
            .iter()
            .filter(|claim| claim.source == Some(source) && claim.kind == kind)
            .fold(0, |sum, claim| sum.saturating_add(claim.amount))
    }
    pub fn reserved_to(&self, sink: SinkEndpoint, kind: ResourceKind) -> u32 {
        self.claims
            .iter()
            .filter(|claim| claim.sink == sink && claim.kind == kind)
            .fold(0, |sum, claim| sum.saturating_add(claim.amount))
    }
    pub fn claim(&mut self, reservation: Reservation) -> bool {
        if self.claims.iter().any(|claim| {
            claim.worker == reservation.worker
                || (claim.task == reservation.task
                    && matches!(reservation.sink, SinkEndpoint::RefineryOutput(_)))
        }) {
            return false;
        }
        self.claims.push(reservation);
        self.claims
            .sort_unstable_by_key(|claim| (claim.worker.to_bits(), claim.task.to_bits()));
        true
    }
    pub fn release_worker(&mut self, worker: Entity) {
        self.claims.retain(|claim| claim.worker != worker);
    }
    pub fn release_task(&mut self, task: Entity) {
        self.claims.retain(|claim| claim.task != task);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct RefiningTask {
    refinery: Entity,
}

impl RefiningTask {
    pub const fn new(refinery: Entity) -> Self {
        Self { refinery }
    }
    pub const fn refinery(self) -> Entity {
        self.refinery
    }
    pub const fn label() -> &'static str {
        "RefineResource"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefiningPhase {
    ToSource,
    Gathering { progress_ticks: u32 },
    ToRefinery,
    Processing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiRefineResource {
    refinery: Entity,
    task: Entity,
    recipe: RecipeKind,
    source: Option<StockEndpoint>,
    phase: RefiningPhase,
}

impl AiRefineResource {
    pub const fn refinery(self) -> Entity {
        self.refinery
    }
    pub const fn task(self) -> Entity {
        self.task
    }
    pub const fn recipe(self) -> RecipeKind {
        self.recipe
    }
    pub const fn phase(self) -> RefiningPhase {
        self.phase
    }
    pub const fn is_actively_processing(self) -> bool {
        matches!(self.phase, RefiningPhase::Processing)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefineryBlockedReason {
    OutputFull,
    NoInput,
    NoEligibleWorker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefiningActivity {
    Saw,
    Stonecut,
    Cook,
}

pub fn npc_refining_activity(world: &World, npc: Entity) -> Option<RefiningActivity> {
    let work = world.get::<AiRefineResource>(npc)?;
    if !work.is_actively_processing() {
        return None;
    }
    let production = world.get::<RefineryProduction>(work.refinery())?;
    if !production.is_active() {
        return None;
    }
    match work.recipe().definition().building() {
        BuildingKind::Sawmill => Some(RefiningActivity::Saw),
        BuildingKind::Stoneworks => Some(RefiningActivity::Stonecut),
        BuildingKind::Kitchen => Some(RefiningActivity::Cook),
        _ => None,
    }
}

impl RefineryBlockedReason {
    pub const fn label(self) -> &'static str {
        match self {
            Self::OutputFull => "Output full",
            Self::NoInput => "No input",
            Self::NoEligibleWorker => "No eligible worker",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefineryStatus {
    pub entity: Entity,
    pub building_kind: BuildingKind,
    pub input_contents: ResourceAmounts,
    pub input_capacity: u32,
    pub output_contents: ResourceAmounts,
    pub output_capacity: u32,
    pub supported_recipes: Vec<RecipeKind>,
    pub current_recipe: Option<RecipeKind>,
    pub progress_ticks: u32,
    pub remaining_ticks: u32,
    pub assigned_worker: Option<Entity>,
    pub blocked_reason: Option<RefineryBlockedReason>,
}

#[derive(Bundle)]
struct RefiningTaskBundle {
    task: Task,
    assignment: TaskAssignment,
    refining: RefiningTask,
}

impl RefiningTaskBundle {
    fn new(refinery: Entity) -> Self {
        Self {
            task: Task,
            assignment: TaskAssignment::unassigned(),
            refining: RefiningTask::new(refinery),
        }
    }
}

pub fn maintain_refining_tasks(
    mut commands: Commands,
    refineries: Query<(Entity, &Building), With<RefineryInventory>>,
    tasks: Query<(Entity, &RefiningTask)>,
) {
    let refinery_entities = refineries
        .iter()
        .filter(|(_, building)| !recipes_for_building(building.kind).is_empty())
        .map(|(entity, _)| entity)
        .collect::<HashSet<_>>();
    let mut represented = HashSet::new();
    let mut existing = tasks.iter().collect::<Vec<_>>();
    existing.sort_unstable_by_key(|(entity, _)| entity.to_bits());
    for (task_entity, task) in existing {
        if !refinery_entities.contains(&task.refinery()) || !represented.insert(task.refinery()) {
            commands.entity(task_entity).despawn();
        }
    }
    let mut missing = refinery_entities.into_iter().collect::<Vec<_>>();
    missing.sort_unstable_by_key(|entity| entity.to_bits());
    for refinery in missing {
        if !represented.contains(&refinery) {
            commands.spawn(RefiningTaskBundle::new(refinery));
        }
    }
}

/// Stable, surface-local refining scheduler. It owns source/output/task claims
/// and deliberately ignores NPC-to-NPC transfers except the worker's own stock.
pub fn assign_refining_work(world: &mut World) {
    if world.get_resource::<ReservationLedger>().is_none() {
        world.insert_resource(ReservationLedger::default());
    }
    cleanup_refining_claims(world);
    let Some(snapshot) = current_navigation_snapshot(world) else {
        return;
    };

    let mut worker_query = world.query_filtered::<(
        Entity,
        &NpcPosition,
        Option<&Sawyer>,
        Option<&Stonemason>,
        Option<&Cook>,
        Option<&AiSearchForFood>,
        Option<&AiGatherResource>,
        Option<&AiConstructBuilding>,
        Option<&AiRefineResource>,
        Option<&AiSeedField>,
        Option<&AiHarvestField>,
        Option<&AiSeedTreePlot>,
        Option<&AiCutTreePlot>,
    ), With<Npc>>();
    let mut workers = worker_query
        .iter(world)
        .filter(
            |(
                _,
                _,
                _,
                _,
                _,
                food,
                gather,
                construction,
                refining,
                seed,
                harvest,
                tree_seed,
                tree_cut,
            )| {
                food.is_none()
                    && gather.is_none()
                    && construction.is_none()
                    && refining.is_none()
                    && seed.is_none()
                    && harvest.is_none()
                    && tree_seed.is_none()
                    && tree_cut.is_none()
            },
        )
        .map(|(entity, position, sawyer, stonemason, cook, ..)| {
            (
                entity,
                *position,
                sawyer.is_some(),
                stonemason.is_some(),
                cook.is_some(),
            )
        })
        .collect::<Vec<_>>();
    workers.sort_unstable_by_key(|(entity, ..)| entity.to_bits());

    let claimed_refineries = world
        .resource::<ReservationLedger>()
        .claims()
        .iter()
        .filter_map(|claim| match claim.sink {
            SinkEndpoint::RefineryOutput(entity) => Some(entity),
            _ => None,
        })
        .collect::<HashSet<_>>();

    let mut task_query = world.query::<(Entity, &RefiningTask)>();
    let tasks = task_query
        .iter(world)
        .map(|(entity, task)| (entity, *task))
        .collect::<Vec<_>>();
    let mut newly_claimed = claimed_refineries;

    for (worker, position, sawyer, stonemason, cook) in workers {
        let mut distances = None;
        let mut candidates = Vec::new();
        for (task_entity, task) in &tasks {
            let refinery = task.refinery();
            if newly_claimed.contains(&refinery) {
                continue;
            }
            let Some(building) = world.get::<Building>(refinery).copied() else {
                continue;
            };
            let Some(inventory) = world.get::<RefineryInventory>(refinery).copied() else {
                continue;
            };
            let production = world
                .get::<RefineryProduction>(refinery)
                .copied()
                .unwrap_or_default();
            let eligible = match building.kind {
                BuildingKind::Sawmill => sawyer,
                BuildingKind::Stoneworks => stonemason,
                BuildingKind::Kitchen => cook,
                _ => false,
            };
            if !eligible || inventory.output_free_size() == 0 {
                continue;
            }
            let distances = distances
                .get_or_insert_with(|| snapshot.distances_from(position.coord))
                .as_ref();
            let Some(distances) = distances else {
                continue;
            };
            let goals = snapshot.exterior_interaction_cells(building.footprint);
            let Some((_, refinery_distance)) = distances.closest_reachable(goals) else {
                continue;
            };
            let recipe_source = if let Some(recipe) = production.recipe() {
                Some((recipe, None))
            } else {
                choose_recipe_and_source(
                    world,
                    &snapshot,
                    distances,
                    worker,
                    refinery,
                    building.kind,
                    inventory,
                )
            };
            let Some((recipe, source)) = recipe_source else {
                continue;
            };
            candidates.push((
                refinery_distance,
                refinery.to_bits(),
                *task_entity,
                refinery,
                recipe,
                source,
            ));
        }
        let Some((_, _, task, refinery, recipe, source)) = candidates
            .into_iter()
            .min_by_key(|candidate| (candidate.0, candidate.1))
        else {
            continue;
        };

        let claim = Reservation {
            worker,
            source,
            sink: SinkEndpoint::RefineryOutput(refinery),
            kind: recipe.definition().output(),
            amount: 1,
            task,
        };
        if !world.resource_mut::<ReservationLedger>().claim(claim) {
            continue;
        }
        newly_claimed.insert(refinery);
        let phase = if source.is_some() {
            RefiningPhase::ToSource
        } else {
            RefiningPhase::ToRefinery
        };
        world.entity_mut(worker).insert(AiRefineResource {
            refinery,
            task,
            recipe,
            source,
            phase,
        });
        if let Some(mut production) = world.get_mut::<RefineryProduction>(refinery) {
            production.assigned_worker = Some(worker);
        }
        if let Some(mut assignment) = world.get_mut::<TaskAssignment>(task) {
            assignment.assign(worker);
        }
    }
}

pub fn route_and_advance_refining_work(world: &mut World) {
    let Some(snapshot) = current_navigation_snapshot(world) else {
        return;
    };
    let mut query = world.query::<(Entity, &NpcPosition, &NpcInventory, &AiRefineResource)>();
    let mut workers = query
        .iter(world)
        .map(|(entity, position, inventory, work)| (entity, *position, *inventory, *work))
        .collect::<Vec<_>>();
    workers.sort_unstable_by_key(|(entity, ..)| entity.to_bits());

    for (worker, position, inventory, mut work) in workers {
        if world.get::<AiSearchForFood>(worker).is_some() {
            release_refining_worker(world, worker, work);
            continue;
        }
        let Some(building) = world.get::<Building>(work.refinery).copied() else {
            release_refining_worker(world, worker, work);
            continue;
        };
        match work.phase {
            RefiningPhase::ToSource => {
                let Some(source) = work.source else {
                    work.phase = RefiningPhase::ToRefinery;
                    world.entity_mut(worker).insert(work);
                    continue;
                };
                let goals = source_interaction_cells(world, &snapshot, source, worker);
                if goals.is_empty() {
                    release_refining_worker(world, worker, work);
                } else if goals.contains(&position.coord)
                    || source == StockEndpoint::NpcInventory(worker)
                {
                    world.entity_mut(worker).remove::<NpcRoute>();
                    world.entity_mut(worker).remove::<MovementTarget>();
                    if matches!(source, StockEndpoint::NaturalNode(_)) {
                        work.phase = RefiningPhase::Gathering { progress_ticks: 0 };
                    } else if withdraw_source(world, source, work.recipe.definition().input(), 1)
                        && world
                            .get_mut::<NpcInventory>(worker)
                            .is_some_and(|mut inv| inv.add(work.recipe.definition().input(), 1))
                    {
                        work.phase = RefiningPhase::ToRefinery;
                    } else {
                        release_refining_worker(world, worker, work);
                        continue;
                    }
                    world.entity_mut(worker).insert(work);
                } else {
                    set_route(world, worker, goals);
                }
            }
            RefiningPhase::Gathering { progress_ticks } => {
                let Some(StockEndpoint::NaturalNode(node_entity)) = work.source else {
                    release_refining_worker(world, worker, work);
                    continue;
                };
                let valid = world.get::<TilePosition>(node_entity).is_some_and(|p| {
                    snapshot
                        .point_interaction_cells(p.coord)
                        .contains(&position.coord)
                }) && world.get::<ResourceNode>(node_entity).is_some_and(|node| {
                    node.quantity > 0 && node.kind == work.recipe.definition().input()
                });
                if !valid {
                    release_refining_worker(world, worker, work);
                    continue;
                }
                let next = progress_ticks.saturating_add(1);
                if next < RESOURCE_GATHER_TICKS_PER_UNIT {
                    work.phase = RefiningPhase::Gathering {
                        progress_ticks: next,
                    };
                    world.entity_mut(worker).insert(work);
                    continue;
                }
                let kind = work.recipe.definition().input();
                let added = world
                    .get_mut::<NpcInventory>(worker)
                    .is_some_and(|mut inv| inv.add(kind, 1));
                if !added {
                    release_refining_worker(world, worker, work);
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
                if let Some(mut skills) = world.get_mut::<NpcSkills>(worker) {
                    if let Some(skill) = SkillKind::try_for_gathered_resource(kind) {
                        skills.add_xp(skill, 1);
                    }
                }
                work.phase = RefiningPhase::ToRefinery;
                world.entity_mut(worker).insert(work);
            }
            RefiningPhase::ToRefinery => {
                let goals = snapshot.exterior_interaction_cells(building.footprint);
                if !goals.contains(&position.coord) {
                    set_route(world, worker, goals);
                    continue;
                }
                world.entity_mut(worker).remove::<NpcRoute>();
                world.entity_mut(worker).remove::<MovementTarget>();
                if world
                    .get::<RefineryProduction>(work.refinery)
                    .is_some_and(|production| production.recipe().is_some())
                {
                    work.phase = RefiningPhase::Processing;
                    world.entity_mut(worker).insert(work);
                    continue;
                }
                let input = work.recipe.definition().input();
                if work.source.is_some() {
                    let can_accept = world
                        .get::<RefineryInventory>(work.refinery)
                        .is_some_and(|inventory| inventory.input_free_size() > 0);
                    let withdrawn = can_accept
                        && world
                            .get_mut::<NpcInventory>(worker)
                            .is_some_and(|mut inv| inv.consume(input, 1));
                    let deposited = withdrawn
                        && world
                            .get_mut::<RefineryInventory>(work.refinery)
                            .is_some_and(|mut inv| inv.add_input(building.kind, input, 1));
                    if !deposited {
                        if withdrawn {
                            let _ = world
                                .get_mut::<NpcInventory>(worker)
                                .is_some_and(|mut inventory| inventory.add(input, 1));
                        }
                        release_refining_worker(world, worker, work);
                        continue;
                    }
                }
                let consumed = world
                    .get_mut::<RefineryInventory>(work.refinery)
                    .is_some_and(|mut inv| inv.consume_input(input, 1));
                if !consumed {
                    release_refining_worker(world, worker, work);
                    continue;
                }
                if let Some(mut production) = world.get_mut::<RefineryProduction>(work.refinery) {
                    if production.recipe.is_none() {
                        production.recipe = Some(work.recipe);
                        production.progress_ticks = 0;
                        production.output_reserved = true;
                    }
                }
                work.phase = RefiningPhase::Processing;
                world.entity_mut(worker).insert(work);
            }
            RefiningPhase::Processing => {
                let goals = snapshot.exterior_interaction_cells(building.footprint);
                if !goals.contains(&position.coord) {
                    work.phase = RefiningPhase::ToRefinery;
                    world.entity_mut(worker).insert(work);
                    continue;
                }
                let Some(mut production) = world.get_mut::<RefineryProduction>(work.refinery)
                else {
                    release_refining_worker(world, worker, work);
                    continue;
                };
                production.progress_ticks = production
                    .progress_ticks
                    .saturating_add(1)
                    .min(REFINING_TICKS_PER_UNIT);
                let recipe = production.recipe.unwrap_or(work.recipe);
                let complete = production.progress_ticks >= REFINING_TICKS_PER_UNIT;
                drop(production);
                if !complete {
                    continue;
                }
                let output = recipe.definition().output();
                let inserted = world
                    .get_mut::<RefineryInventory>(work.refinery)
                    .is_some_and(|mut inv| inv.add_output(building.kind, output, 1));
                if !inserted {
                    continue;
                }
                if let Some(mut skills) = world.get_mut::<NpcSkills>(worker) {
                    skills.add_xp(recipe.definition().skill(), 1);
                }
                if let Some(mut production) = world.get_mut::<RefineryProduction>(work.refinery) {
                    *production = RefineryProduction::default();
                }
                release_refining_worker(world, worker, work);
            }
        }
        let _ = inventory;
    }
}

pub fn refinery_status(world: &World, entity: Entity) -> Option<RefineryStatus> {
    let building = world.get::<Building>(entity)?;
    let inventory = world.get::<RefineryInventory>(entity)?;
    let production = world
        .get::<RefineryProduction>(entity)
        .copied()
        .unwrap_or_default();
    let output_full = inventory.output_free_size() == 0;
    let no_input = production.recipe().is_none()
        && recipes_for_building(building.kind).iter().all(|recipe| {
            available_stock_readonly(world, recipe.definition().input(), entity) == 0
        });
    let no_worker = !has_eligible_worker(world, building.kind);
    let blocked_reason = if output_full {
        Some(RefineryBlockedReason::OutputFull)
    } else if no_input {
        Some(RefineryBlockedReason::NoInput)
    } else if no_worker {
        Some(RefineryBlockedReason::NoEligibleWorker)
    } else {
        None
    };
    Some(RefineryStatus {
        entity,
        building_kind: building.kind,
        input_contents: inventory.input_contents(),
        input_capacity: inventory.input_capacity(),
        output_contents: inventory.output_contents(),
        output_capacity: inventory.output_capacity(),
        supported_recipes: recipes_for_building(building.kind).to_vec(),
        current_recipe: production.recipe(),
        progress_ticks: production.progress_ticks(),
        remaining_ticks: production.remaining_ticks(),
        assigned_worker: production.assigned_worker(),
        blocked_reason,
    })
}

fn choose_recipe_and_source(
    world: &mut World,
    snapshot: &NavigationSnapshot,
    distances: &NavigationDistances,
    worker: Entity,
    refinery: Entity,
    building: BuildingKind,
    inventory: RefineryInventory,
) -> Option<(RecipeKind, Option<StockEndpoint>)> {
    for recipe in recipes_for_building(building) {
        if inventory.input_contents().get(recipe.definition().input()) > 0 {
            return Some((*recipe, None));
        }
    }
    let mut candidates = Vec::new();
    for recipe in recipes_for_building(building) {
        let kind = recipe.definition().input();
        for source in stock_sources(world, kind, refinery, worker) {
            let reserved = world
                .resource::<ReservationLedger>()
                .reserved_from(source, kind);
            if source_stock(world, source, kind).saturating_sub(reserved) == 0 {
                continue;
            }
            let distance = if source == StockEndpoint::NpcInventory(worker) {
                0
            } else {
                let goals = source_interaction_cells(world, snapshot, source, worker);
                let Some((_, distance)) = distances.closest_reachable(goals) else {
                    continue;
                };
                distance
            };
            candidates.push((
                distance,
                endpoint_entity(source).to_bits(),
                kind as usize,
                *recipe,
                source,
            ));
        }
    }
    candidates
        .into_iter()
        .min_by_key(|candidate| (candidate.0, candidate.1, candidate.2))
        .map(|(_, _, _, recipe, source)| (recipe, Some(source)))
}

pub(crate) fn stock_sources(
    world: &mut World,
    kind: ResourceKind,
    exclude_refinery: Entity,
    worker: Entity,
) -> Vec<StockEndpoint> {
    let mut sources = Vec::new();
    if world
        .get::<NpcInventory>(worker)
        .is_some_and(|inv| inv.contents().get(kind) > 0)
    {
        sources.push(StockEndpoint::NpcInventory(worker));
    }
    if let Some(mut query) = world.try_query::<(Entity, &ResourceNode)>() {
        sources.extend(query.iter(world).filter_map(|(entity, node)| {
            (entity != exclude_refinery && node.kind == kind && node.quantity > 0)
                .then_some(StockEndpoint::NaturalNode(entity))
        }));
    }
    if let Some(mut query) = world.try_query::<(Entity, &WarehouseInventory)>() {
        sources.extend(query.iter(world).filter_map(|(entity, inventory)| {
            (entity != exclude_refinery && inventory.contents().get(kind) > 0)
                .then_some(StockEndpoint::Warehouse(entity))
        }));
    }
    if let Some(mut query) = world.try_query::<(Entity, &FarmInventory)>() {
        sources.extend(query.iter(world).filter_map(|(entity, inventory)| {
            (entity != exclude_refinery && inventory.contents().get(kind) > 0)
                .then_some(StockEndpoint::Farm(entity))
        }));
    }
    if let Some(mut query) = world.try_query::<(Entity, &ForesterLodgeInventory)>() {
        sources.extend(query.iter(world).filter_map(|(entity, inventory)| {
            (entity != exclude_refinery && inventory.contents().get(kind) > 0)
                .then_some(StockEndpoint::ForesterLodge(entity))
        }));
    }
    if let Some(mut query) = world.try_query::<(Entity, &RefineryInventory)>() {
        for (entity, inv) in query
            .iter(world)
            .filter(|(entity, _)| *entity != exclude_refinery)
        {
            if inv.input_contents().get(kind) > 0 {
                sources.push(StockEndpoint::RefineryInput(entity));
            }
            if inv.output_contents().get(kind) > 0 {
                sources.push(StockEndpoint::RefineryOutput(entity));
            }
        }
    }
    sources.sort_unstable_by_key(|source| {
        (endpoint_entity(*source).to_bits(), endpoint_order(*source))
    });
    sources
}

pub(crate) fn source_interaction_cells(
    world: &World,
    snapshot: &NavigationSnapshot,
    source: StockEndpoint,
    worker: Entity,
) -> Vec<crate::grid::CellCoord> {
    if source == StockEndpoint::NpcInventory(worker) {
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
            .get::<Building>(endpoint_entity(source))
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
        StockEndpoint::NpcInventory(entity) => world
            .get::<NpcInventory>(entity)
            .map_or(0, |inv| inv.contents().get(kind)),
        StockEndpoint::Warehouse(entity) => world
            .get::<WarehouseInventory>(entity)
            .map_or(0, |inv| inv.contents().get(kind)),
        StockEndpoint::Farm(entity) => world
            .get::<FarmInventory>(entity)
            .map_or(0, |inv| inv.contents().get(kind)),
        StockEndpoint::ForesterLodge(entity) => world
            .get::<ForesterLodgeInventory>(entity)
            .map_or(0, |inv| inv.contents().get(kind)),
        StockEndpoint::RefineryInput(entity) => world
            .get::<RefineryInventory>(entity)
            .map_or(0, |inv| inv.input_contents().get(kind)),
        StockEndpoint::RefineryOutput(entity) => world
            .get::<RefineryInventory>(entity)
            .map_or(0, |inv| inv.output_contents().get(kind)),
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
        StockEndpoint::NpcInventory(entity) => world
            .get_mut::<NpcInventory>(entity)
            .is_some_and(|mut inv| inv.consume(kind, amount)),
        StockEndpoint::Warehouse(entity) => world
            .get_mut::<WarehouseInventory>(entity)
            .is_some_and(|mut inv| inv.consume(kind, amount)),
        StockEndpoint::Farm(entity) => world
            .get_mut::<FarmInventory>(entity)
            .is_some_and(|mut inv| inv.consume(kind, amount)),
        StockEndpoint::ForesterLodge(entity) => world
            .get_mut::<ForesterLodgeInventory>(entity)
            .is_some_and(|mut inv| inv.consume(kind, amount)),
        StockEndpoint::RefineryInput(entity) => world
            .get_mut::<RefineryInventory>(entity)
            .is_some_and(|mut inv| inv.consume_input(kind, amount)),
        StockEndpoint::RefineryOutput(entity) => world
            .get_mut::<RefineryInventory>(entity)
            .is_some_and(|mut inv| inv.consume_output(kind, amount)),
    }
}

pub(crate) fn endpoint_entity(endpoint: StockEndpoint) -> Entity {
    match endpoint {
        StockEndpoint::NaturalNode(entity)
        | StockEndpoint::NpcInventory(entity)
        | StockEndpoint::Warehouse(entity)
        | StockEndpoint::Farm(entity)
        | StockEndpoint::ForesterLodge(entity)
        | StockEndpoint::RefineryInput(entity)
        | StockEndpoint::RefineryOutput(entity) => entity,
    }
}

fn endpoint_order(endpoint: StockEndpoint) -> u8 {
    match endpoint {
        StockEndpoint::NaturalNode(_) => 0,
        StockEndpoint::NpcInventory(_) => 1,
        StockEndpoint::Warehouse(_) => 2,
        StockEndpoint::Farm(_) => 3,
        StockEndpoint::ForesterLodge(_) => 4,
        StockEndpoint::RefineryInput(_) => 5,
        StockEndpoint::RefineryOutput(_) => 6,
    }
}

fn set_route(world: &mut World, worker: Entity, goals: Vec<crate::grid::CellCoord>) {
    let matches = world
        .get::<NpcRoute>(worker)
        .is_some_and(|route| route.goals() == goals.as_slice());
    if !matches {
        world.entity_mut(worker).insert(NpcRoute::new(goals));
        world.entity_mut(worker).remove::<MovementTarget>();
    }
}

fn release_refining_worker(world: &mut World, worker: Entity, work: AiRefineResource) {
    world
        .resource_mut::<ReservationLedger>()
        .release_task(work.task);
    world.entity_mut(worker).remove::<AiRefineResource>();
    world.entity_mut(worker).remove::<NpcRoute>();
    world.entity_mut(worker).remove::<MovementTarget>();
    if let Some(mut production) = world.get_mut::<RefineryProduction>(work.refinery) {
        production.assigned_worker = None;
    }
    if let Some(mut assignment) = world.get_mut::<TaskAssignment>(work.task) {
        assignment.clear();
    }
}

fn cleanup_refining_claims(world: &mut World) {
    let mut query = world.query::<(Entity, &AiRefineResource)>();
    let active = query
        .iter(world)
        .map(|(entity, work)| (entity, *work))
        .collect::<HashMap<_, _>>();
    let stale = world
        .resource::<ReservationLedger>()
        .claims()
        .iter()
        .filter(|claim| matches!(claim.sink, SinkEndpoint::RefineryOutput(_)))
        .filter(|claim| {
            active
                .get(&claim.worker)
                .is_none_or(|work| work.task != claim.task || world.get_entity(claim.task).is_err())
        })
        .map(|claim| claim.task)
        .collect::<Vec<_>>();
    for task in stale {
        world.resource_mut::<ReservationLedger>().release_task(task);
    }
    let interrupted = active
        .into_iter()
        .filter(|(worker, _)| world.get::<AiSearchForFood>(*worker).is_some())
        .collect::<Vec<_>>();
    for (worker, work) in interrupted {
        release_refining_worker(world, worker, work);
    }
}

fn available_stock_readonly(world: &World, kind: ResourceKind, exclude: Entity) -> u32 {
    let mut total = 0u32;
    if let Some(mut query) = world.try_query::<(Entity, &ResourceNode)>() {
        for (_, node) in query
            .iter(world)
            .filter(|(entity, node)| *entity != exclude && node.kind == kind)
        {
            total = total.saturating_add(node.quantity);
        }
    }
    if let Some(mut query) = world.try_query::<(Entity, &WarehouseInventory)>() {
        for (_, inventory) in query.iter(world).filter(|(entity, _)| *entity != exclude) {
            total = total.saturating_add(inventory.contents().get(kind));
        }
    }
    if let Some(mut query) = world.try_query::<(Entity, &FarmInventory)>() {
        for (_, inventory) in query.iter(world).filter(|(entity, _)| *entity != exclude) {
            total = total.saturating_add(inventory.contents().get(kind));
        }
    }
    if let Some(mut query) = world.try_query::<(Entity, &ForesterLodgeInventory)>() {
        for (_, inventory) in query.iter(world).filter(|(entity, _)| *entity != exclude) {
            total = total.saturating_add(inventory.contents().get(kind));
        }
    }
    if let Some(mut query) = world.try_query::<(Entity, &RefineryInventory)>() {
        for (entity, inv) in query.iter(world) {
            total = total.saturating_add(inv.input_contents().get(kind));
            if entity != exclude {
                total = total.saturating_add(inv.output_contents().get(kind));
            }
        }
    }
    total
}

fn has_eligible_worker(world: &World, building: BuildingKind) -> bool {
    world
        .try_query::<(&Npc, Option<&Sawyer>, Option<&Stonemason>, Option<&Cook>)>()
        .is_some_and(|mut query| {
            query
                .iter(world)
                .any(|(_, sawyer, stonemason, cook)| match building {
                    BuildingKind::Sawmill => sawyer.is_some(),
                    BuildingKind::Stoneworks => stonemason.is_some(),
                    BuildingKind::Kitchen => cook.is_some(),
                    _ => false,
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::BuildingFootprint;
    use crate::grid::{CellCoord, Grid, GridSize};
    use crate::tile::{TileBundle, TileIndex};

    #[test]
    fn source_selection_skips_unreachable_stock() {
        let mut world = navigation_world();
        let worker = world
            .spawn((
                NpcPosition::new(CellCoord::new(0, 0)),
                NpcInventory::default(),
            ))
            .id();
        let refinery = world.spawn_empty().id();
        let reachable = spawn_warehouse(&mut world, CellCoord::new(2, 0), ResourceKind::Wood, 1);
        let unreachable = spawn_warehouse(&mut world, CellCoord::new(6, 6), ResourceKind::Wood, 1);
        for coord in [
            CellCoord::new(6, 5),
            CellCoord::new(5, 6),
            CellCoord::new(7, 6),
            CellCoord::new(6, 7),
        ] {
            let tile = world.resource::<TileIndex>().get(coord).unwrap();
            world.entity_mut(tile).insert(ResourceNode {
                kind: ResourceKind::Stone,
                quantity: 1,
            });
        }
        let snapshot = NavigationSnapshot::from_world(&world).unwrap();
        let distances = snapshot.distances_from(CellCoord::new(0, 0)).unwrap();

        let selected = choose_recipe_and_source(
            &mut world,
            &snapshot,
            &distances,
            worker,
            refinery,
            BuildingKind::Sawmill,
            RefineryInventory::empty(),
        );

        assert_eq!(
            selected,
            Some((
                RecipeKind::SawWood,
                Some(StockEndpoint::Warehouse(reachable))
            ))
        );
        assert_ne!(reachable, unreachable);
    }

    #[test]
    fn equidistant_sources_preserve_entity_id_tie_break() {
        let mut world = navigation_world();
        let worker = world
            .spawn((
                NpcPosition::new(CellCoord::new(4, 4)),
                NpcInventory::default(),
            ))
            .id();
        let refinery = world.spawn_empty().id();
        let first = spawn_warehouse(&mut world, CellCoord::new(2, 4), ResourceKind::Wood, 1);
        let second = spawn_warehouse(&mut world, CellCoord::new(6, 4), ResourceKind::Wood, 1);
        let lower_entity = if first.to_bits() < second.to_bits() {
            first
        } else {
            second
        };
        let snapshot = NavigationSnapshot::from_world(&world).unwrap();
        let distances = snapshot.distances_from(CellCoord::new(4, 4)).unwrap();
        let first_distance = distances
            .closest_reachable(source_interaction_cells(
                &world,
                &snapshot,
                StockEndpoint::Warehouse(first),
                worker,
            ))
            .unwrap()
            .1;
        let second_distance = distances
            .closest_reachable(source_interaction_cells(
                &world,
                &snapshot,
                StockEndpoint::Warehouse(second),
                worker,
            ))
            .unwrap()
            .1;
        assert_eq!((first_distance, second_distance), (1, 1));

        let selected = choose_recipe_and_source(
            &mut world,
            &snapshot,
            &distances,
            worker,
            refinery,
            BuildingKind::Sawmill,
            RefineryInventory::empty(),
        );

        assert_eq!(
            selected,
            Some((
                RecipeKind::SawWood,
                Some(StockEndpoint::Warehouse(lower_entity))
            ))
        );
    }

    #[test]
    fn stock_sources_query_each_supported_inventory_archetype_in_stable_order() {
        let mut world = World::new();
        for _ in 0..1_000 {
            world.spawn_empty();
        }

        let worker = world
            .spawn(NpcInventory::new(ResourceAmounts::of(
                ResourceKind::Wood,
                1,
            )))
            .id();
        let natural_node = world
            .spawn(ResourceNode {
                kind: ResourceKind::Wood,
                quantity: 2,
            })
            .id();
        let warehouse = world.spawn(warehouse_inventory(ResourceKind::Wood, 3)).id();
        let lodge = world.spawn(forester_inventory(4)).id();
        let refinery = world
            .spawn(refinery_inventory(
                ResourceAmounts::of(ResourceKind::Wood, 5),
                ResourceAmounts::of(ResourceKind::Wood, 6),
            ))
            .id();
        let excluded_refinery = world
            .spawn(refinery_inventory(
                ResourceAmounts::of(ResourceKind::Wood, 7),
                ResourceAmounts::of(ResourceKind::Wood, 8),
            ))
            .id();
        world.spawn(WarehouseInventory::empty());

        let mut expected = vec![
            StockEndpoint::NpcInventory(worker),
            StockEndpoint::NaturalNode(natural_node),
            StockEndpoint::Warehouse(warehouse),
            StockEndpoint::ForesterLodge(lodge),
            StockEndpoint::RefineryInput(refinery),
            StockEndpoint::RefineryOutput(refinery),
        ];
        expected.sort_unstable_by_key(|source| {
            (endpoint_entity(*source).to_bits(), endpoint_order(*source))
        });

        assert_eq!(
            stock_sources(&mut world, ResourceKind::Wood, excluded_refinery, worker,),
            expected
        );
    }

    #[test]
    fn stock_sources_query_farm_inventory_and_ignore_empty_inventory() {
        let mut world = World::new();
        let worker = world.spawn(NpcInventory::default()).id();
        let mut farm_inventory = FarmInventory::empty();
        assert!(farm_inventory.add_crops(12));
        let farm = world.spawn(farm_inventory).id();
        world.spawn(FarmInventory::empty());

        assert_eq!(
            stock_sources(&mut world, ResourceKind::Crops, Entity::PLACEHOLDER, worker,),
            vec![StockEndpoint::Farm(farm)]
        );
    }

    #[test]
    fn available_stock_queries_only_relevant_components_and_preserves_exclusion() {
        let mut world = World::new();
        for _ in 0..1_000 {
            world.spawn_empty();
        }
        world.spawn(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 2,
        });
        world.spawn(warehouse_inventory(ResourceKind::Wood, 3));
        world.spawn(forester_inventory(4));
        world.spawn(NpcInventory::new(ResourceAmounts::of(
            ResourceKind::Wood,
            100,
        )));
        world.spawn(refinery_inventory(
            ResourceAmounts::of(ResourceKind::Wood, 5),
            ResourceAmounts::of(ResourceKind::Wood, 6),
        ));
        let excluded = world
            .spawn(refinery_inventory(
                ResourceAmounts::of(ResourceKind::Wood, 7),
                ResourceAmounts::of(ResourceKind::Wood, 8),
            ))
            .id();

        assert_eq!(
            available_stock_readonly(&world, ResourceKind::Wood, excluded),
            27
        );
    }

    #[test]
    fn available_stock_query_saturates() {
        let mut world = World::new();
        world.spawn(ResourceNode {
            kind: ResourceKind::Stone,
            quantity: u32::MAX,
        });
        world.spawn(ResourceNode {
            kind: ResourceKind::Stone,
            quantity: 1,
        });

        assert_eq!(
            available_stock_readonly(&world, ResourceKind::Stone, Entity::PLACEHOLDER),
            u32::MAX
        );
    }

    #[test]
    fn eligible_worker_query_requires_npc_and_matching_skill() {
        let mut world = World::new();
        world.spawn((Sawyer, Stonemason, Cook));
        world.spawn(Npc);

        assert!(!has_eligible_worker(&world, BuildingKind::Sawmill));
        assert!(!has_eligible_worker(&world, BuildingKind::Stoneworks));
        assert!(!has_eligible_worker(&world, BuildingKind::Kitchen));

        world.spawn((Npc, Sawyer));
        world.spawn((Npc, Stonemason));
        world.spawn((Npc, Cook));

        assert!(has_eligible_worker(&world, BuildingKind::Sawmill));
        assert!(has_eligible_worker(&world, BuildingKind::Stoneworks));
        assert!(has_eligible_worker(&world, BuildingKind::Kitchen));
        assert!(!has_eligible_worker(&world, BuildingKind::Warehouse));
    }

    fn warehouse_inventory(kind: ResourceKind, amount: u32) -> WarehouseInventory {
        let mut inventory = WarehouseInventory::empty();
        assert!(inventory.add(kind, amount));
        inventory
    }

    fn forester_inventory(wood: u32) -> ForesterLodgeInventory {
        let mut inventory = ForesterLodgeInventory::empty();
        assert!(inventory.add_wood(wood));
        inventory
    }

    fn refinery_inventory(input: ResourceAmounts, output: ResourceAmounts) -> RefineryInventory {
        RefineryInventory {
            input: ResourceInventory::new(input, REFINERY_INPUT_CAPACITY),
            output: ResourceInventory::new(output, REFINERY_OUTPUT_CAPACITY),
        }
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

    fn spawn_warehouse(
        world: &mut World,
        coord: CellCoord,
        kind: ResourceKind,
        amount: u32,
    ) -> Entity {
        world
            .spawn((
                Building::new(BuildingKind::Warehouse, BuildingFootprint::new(coord, 1, 1)),
                warehouse_inventory(kind, amount),
            ))
            .id()
    }
}
