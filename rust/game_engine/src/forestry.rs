use crate::buildings::{
    assign_default_building_name, validate_building_footprint_placement, Building,
    BuildingBlueprint, BuildingFootprint, BuildingKind, BuildingPlacementError,
};
use crate::farming::FIELD_SEEDING_TICKS;
use crate::grid::CellCoord;
use crate::plots::{cardinal_neighbors, connected_cells, PlotGrowth};
use crate::resources::{ResourceAmounts, ResourceInventory, ResourceKind};
use crate::skills::{NpcSkills, SkillKind};
use crate::time::SIMULATION_TICKS_PER_YEAR;
use bevy_ecs::prelude::*;
use std::collections::{HashMap, HashSet};

pub const MAX_TREE_PLOTS_PER_FORESTER_LODGE: usize = 200;
pub const FORESTER_LODGE_INVENTORY_MAX_WOOD: u32 = 200;
pub const TREE_PLOT_SEEDING_TICKS: u32 = FIELD_SEEDING_TICKS * 5;
pub const TREE_PLOT_GROWTH_TICKS: u32 = SIMULATION_TICKS_PER_YEAR * 5;
pub const TREE_PLOT_CUTTING_TICKS: u32 = 60;
pub const TREE_PLOT_WOOD_YIELD: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Forester;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct ForesterLodgeInventory {
    inventory: ResourceInventory,
}

impl ForesterLodgeInventory {
    pub const fn empty() -> Self {
        Self {
            inventory: ResourceInventory::empty(FORESTER_LODGE_INVENTORY_MAX_WOOD),
        }
    }

    pub const fn contents(self) -> ResourceAmounts {
        self.inventory.contents()
    }

    pub const fn max_size(self) -> u32 {
        self.inventory.max_size()
    }

    pub const fn used_size(self) -> u32 {
        self.inventory.used_size()
    }

    pub const fn free_size(self) -> u32 {
        self.inventory.free_size()
    }

    pub const fn wood(self) -> u32 {
        self.inventory.contents().get(ResourceKind::Wood)
    }

    pub const fn has_wood_capacity(self) -> bool {
        self.free_size() >= TREE_PLOT_WOOD_YIELD
    }

    pub fn add_wood(&mut self, amount: u32) -> bool {
        self.inventory.add(ResourceKind::Wood, amount)
    }

    pub fn consume_wood(&mut self, amount: u32) -> bool {
        self.inventory.consume(ResourceKind::Wood, amount)
    }

    pub fn consume(&mut self, kind: ResourceKind, amount: u32) -> bool {
        self.inventory.consume(kind, amount)
    }
}

impl Default for ForesterLodgeInventory {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct TreePlotOwner {
    forester_lodge: Entity,
}

impl TreePlotOwner {
    pub const fn new(forester_lodge: Entity) -> Self {
        Self { forester_lodge }
    }

    pub const fn lodge(self) -> Entity {
        self.forester_lodge
    }

    pub const fn forester_lodge(self) -> Entity {
        self.forester_lodge
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct TreePlotGrowth {
    growth: PlotGrowth,
}

impl TreePlotGrowth {
    pub const fn seedable() -> Self {
        Self {
            growth: PlotGrowth::seedable(),
        }
    }

    pub const fn with_seeding_progress(seeding_progress_ticks: u32) -> Self {
        Self {
            growth: PlotGrowth::with_seeding_progress(seeding_progress_ticks),
        }
    }

    pub const fn growing(growth_ticks: u32) -> Self {
        Self {
            growth: PlotGrowth::growing(growth_ticks, TREE_PLOT_SEEDING_TICKS),
        }
    }

    pub const fn seeding_progress_ticks(self) -> u32 {
        self.growth.seeding_progress_ticks()
    }

    pub const fn growth_ticks(self) -> Option<u32> {
        self.growth.growth_ticks()
    }

    pub const fn is_seedable(self) -> bool {
        self.growth.is_seedable(TREE_PLOT_SEEDING_TICKS)
    }

    pub const fn is_mature(self) -> bool {
        matches!(self.growth.growth_ticks(), Some(ticks) if ticks >= TREE_PLOT_GROWTH_TICKS)
    }

    pub fn advance_seeding_tick(&mut self) -> bool {
        self.growth.advance_seeding_tick(TREE_PLOT_SEEDING_TICKS)
    }

    pub fn advance_growth_tick(&mut self) {
        self.growth.advance_growth_tick(TREE_PLOT_GROWTH_TICKS);
    }

    pub fn reset_after_cut(&mut self) {
        self.growth.reset();
    }

    pub fn state(self, lodge_active: bool, actively_seeding: bool) -> TreePlotState {
        if !lodge_active {
            return TreePlotState::Inactive;
        }
        if actively_seeding && self.is_seedable() {
            return TreePlotState::Seeding;
        }
        match self.growth.growth_ticks() {
            None => TreePlotState::Seedable,
            Some(ticks) if ticks >= TREE_PLOT_GROWTH_TICKS => TreePlotState::Mature,
            Some(ticks) if ticks >= TREE_PLOT_GROWTH_TICKS / 2 => TreePlotState::Young,
            Some(_) => TreePlotState::Sapling,
        }
    }
}

impl Default for TreePlotGrowth {
    fn default() -> Self {
        Self::seedable()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreePlotState {
    Inactive,
    Seedable,
    Seeding,
    Sapling,
    Young,
    Mature,
}

impl TreePlotState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::Seedable => "Seedable",
            Self::Seeding => "Seeding",
            Self::Sapling => "Sapling",
            Self::Young => "Young",
            Self::Mature => "Mature",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreePlotPlacementError {
    OutOfBounds,
    OverlapsBuilding,
    InvalidTerrain,
    BlockedByResourceNode,
    OwnerMissing,
    OwnerNotForesterLodge,
    NotConnected,
    ForesterLodgeTreePlotLimitReached,
}

impl From<BuildingPlacementError> for TreePlotPlacementError {
    fn from(value: BuildingPlacementError) -> Self {
        match value {
            BuildingPlacementError::OutOfBounds => Self::OutOfBounds,
            BuildingPlacementError::OverlapsBuilding => Self::OverlapsBuilding,
            BuildingPlacementError::InvalidTerrain => Self::InvalidTerrain,
            BuildingPlacementError::BlockedByResourceNode => Self::BlockedByResourceNode,
            BuildingPlacementError::FieldRequiresFarm
            | BuildingPlacementError::TreePlotRequiresLodge => Self::OwnerMissing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreePlotPlacementPreview {
    pub coord: CellCoord,
    pub result: Result<BuildingFootprint, TreePlotPlacementError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedTreePlot {
    pub coord: CellCoord,
    pub entity: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedTreePlotPlacement {
    pub coord: CellCoord,
    pub error: TreePlotPlacementError,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreePlotPlacementBatchResult {
    pub placed: Vec<PlacedTreePlot>,
    pub rejected: Vec<RejectedTreePlotPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct SeedTreePlot {
    tree_plot: Entity,
}

impl SeedTreePlot {
    pub const fn new(tree_plot: Entity) -> Self {
        Self { tree_plot }
    }

    pub const fn tree_plot(self) -> Entity {
        self.tree_plot
    }

    pub const fn label() -> &'static str {
        "SeedTreePlot"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct CutTreePlot {
    tree_plot: Entity,
}

impl CutTreePlot {
    pub const fn new(tree_plot: Entity) -> Self {
        Self { tree_plot }
    }

    pub const fn tree_plot(self) -> Entity {
        self.tree_plot
    }

    pub const fn label() -> &'static str {
        "CutTreePlot"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiSeedTreePlot {
    tree_plot: Entity,
}

impl AiSeedTreePlot {
    pub const fn new(tree_plot: Entity) -> Self {
        Self { tree_plot }
    }

    pub const fn tree_plot(self) -> Entity {
        self.tree_plot
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiCutTreePlot {
    tree_plot: Entity,
    progress_ticks: u32,
}

impl AiCutTreePlot {
    pub const fn new(tree_plot: Entity) -> Self {
        Self {
            tree_plot,
            progress_ticks: 0,
        }
    }

    pub const fn tree_plot(self) -> Entity {
        self.tree_plot
    }

    pub const fn progress_ticks(self) -> u32 {
        self.progress_ticks
    }

    pub fn advance_tick(&mut self) {
        self.progress_ticks = self.progress_ticks.saturating_add(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Bundle)]
struct SeedTreePlotTaskBundle {
    task: crate::tasks::Task,
    seed: SeedTreePlot,
}

impl SeedTreePlotTaskBundle {
    const fn new(tree_plot: Entity) -> Self {
        Self {
            task: crate::tasks::Task,
            seed: SeedTreePlot::new(tree_plot),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Bundle)]
struct CutTreePlotTaskBundle {
    task: crate::tasks::Task,
    cut: CutTreePlot,
}

impl CutTreePlotTaskBundle {
    const fn new(tree_plot: Entity) -> Self {
        Self {
            task: crate::tasks::Task,
            cut: CutTreePlot::new(tree_plot),
        }
    }
}

pub fn validate_tree_plot_blueprint_placement(
    world: &World,
    forester_lodge: Entity,
    coord: CellCoord,
) -> Result<BuildingFootprint, TreePlotPlacementError> {
    let lodge_footprint = forester_lodge_footprint(world, forester_lodge)?;
    if linked_tree_plot_count(world, forester_lodge) >= MAX_TREE_PLOTS_PER_FORESTER_LODGE {
        return Err(TreePlotPlacementError::ForesterLodgeTreePlotLimitReached);
    }

    let footprint = validate_tree_plot_footprint(world, coord)?;
    if !is_tree_plot_connected_to_network(
        world,
        forester_lodge,
        coord,
        lodge_footprint,
        &HashSet::new(),
    ) {
        return Err(TreePlotPlacementError::NotConnected);
    }
    Ok(footprint)
}

pub fn validate_tree_plot_blueprint_placement_batch(
    world: &World,
    forester_lodge: Entity,
    coords: impl IntoIterator<Item = CellCoord>,
) -> Vec<TreePlotPlacementPreview> {
    tree_plot_batch_preview(world, forester_lodge, coords)
}

pub fn place_tree_plot_blueprint(
    world: &mut World,
    forester_lodge: Entity,
    coord: CellCoord,
) -> Result<Entity, TreePlotPlacementError> {
    let footprint = validate_tree_plot_blueprint_placement(world, forester_lodge, coord)?;
    let entity = world
        .spawn((
            crate::buildings::BuildingBlueprintBundle::new(BuildingKind::TreePlot, footprint),
            TreePlotOwner::new(forester_lodge),
        ))
        .id();
    assign_default_building_name(world, entity, BuildingKind::TreePlot);
    Ok(entity)
}

pub fn place_tree_plot_blueprints(
    world: &mut World,
    forester_lodge: Entity,
    coords: impl IntoIterator<Item = CellCoord>,
) -> TreePlotPlacementBatchResult {
    let previews = tree_plot_batch_preview(world, forester_lodge, coords);
    let mut result = TreePlotPlacementBatchResult::default();
    for preview in previews {
        match preview.result {
            Ok(footprint) => {
                let entity = world
                    .spawn((
                        crate::buildings::BuildingBlueprintBundle::new(
                            BuildingKind::TreePlot,
                            footprint,
                        ),
                        TreePlotOwner::new(forester_lodge),
                    ))
                    .id();
                assign_default_building_name(world, entity, BuildingKind::TreePlot);
                result.placed.push(PlacedTreePlot {
                    coord: preview.coord,
                    entity,
                });
            }
            Err(error) => result.rejected.push(RejectedTreePlotPlacement {
                coord: preview.coord,
                error,
            }),
        }
    }
    result
}

pub fn tree_plot_state(world: &World, tree_plot: Entity) -> Option<TreePlotState> {
    let growth = *world.get::<TreePlotGrowth>(tree_plot)?;
    let owner = *world.get::<TreePlotOwner>(tree_plot)?;
    let lodge_active = constructed_forester_lodge_inventory(world, owner.lodge()).is_some();
    let actively_seeding = active_seed_tree_plots(world).contains(&tree_plot);
    Some(growth.state(lodge_active, actively_seeding))
}

pub fn forester_lodge_tree_plot_counts(world: &World, forester_lodge: Entity) -> (usize, usize) {
    let mut linked = 0;
    let mut constructed = 0;
    for (_, state, owner) in linked_tree_plot_states(world) {
        if owner.lodge() == forester_lodge {
            linked += 1;
            if state == TreePlotEntityState::Constructed {
                constructed += 1;
            }
        }
    }
    (linked, constructed)
}

pub fn system_advance_tree_growth(mut tree_plots: Query<&mut TreePlotGrowth>) {
    for mut growth in &mut tree_plots {
        growth.advance_growth_tick();
    }
}

pub fn maintain_forestry_tasks(
    mut commands: Commands,
    tree_plots: Query<(Entity, &Building, &TreePlotOwner, &TreePlotGrowth)>,
    lodges: Query<(&Building, &ForesterLodgeInventory)>,
    seed_tasks: Query<(Entity, &SeedTreePlot)>,
    cut_tasks: Query<(Entity, &CutTreePlot)>,
) {
    let mut seedable = HashSet::new();
    let mut cuttable = HashSet::new();
    for (entity, building, owner, growth) in &tree_plots {
        if building.kind != BuildingKind::TreePlot {
            continue;
        }
        let Ok((lodge, inventory)) = lodges.get(owner.lodge()) else {
            continue;
        };
        if lodge.kind != BuildingKind::ForesterLodge {
            continue;
        }
        if growth.is_seedable() {
            seedable.insert(entity);
        }
        if growth.is_mature() && inventory.has_wood_capacity() {
            cuttable.insert(entity);
        }
    }

    let mut represented_seed = HashSet::new();
    for (task_entity, task) in &seed_tasks {
        let plot = task.tree_plot();
        if !seedable.contains(&plot) || !represented_seed.insert(plot) {
            commands.entity(task_entity).despawn();
        }
    }
    let mut represented_cut = HashSet::new();
    for (task_entity, task) in &cut_tasks {
        let plot = task.tree_plot();
        if !cuttable.contains(&plot) || !represented_cut.insert(plot) {
            commands.entity(task_entity).despawn();
        }
    }
    for plot in seedable {
        if !represented_seed.contains(&plot) {
            commands.spawn(SeedTreePlotTaskBundle::new(plot));
        }
    }
    for plot in cuttable {
        if !represented_cut.contains(&plot) {
            commands.spawn(CutTreePlotTaskBundle::new(plot));
        }
    }
}

pub fn tree_plot_seeding_is_actionable(
    tree_plot: Entity,
    tree_plots: &Query<(Entity, &Building, &TreePlotOwner, &TreePlotGrowth)>,
    lodges: &Query<(&Building, &ForesterLodgeInventory)>,
) -> Option<CellCoord> {
    let Ok((_, building, owner, growth)) = tree_plots.get(tree_plot) else {
        return None;
    };
    if building.kind != BuildingKind::TreePlot || !growth.is_seedable() {
        return None;
    }
    let Ok((lodge, _)) = lodges.get(owner.lodge()) else {
        return None;
    };
    (lodge.kind == BuildingKind::ForesterLodge).then_some(building.footprint.origin())
}

pub fn tree_plot_cutting_is_actionable(
    tree_plot: Entity,
    tree_plots: &Query<(Entity, &Building, &TreePlotOwner, &TreePlotGrowth)>,
    lodges: &Query<(&Building, &ForesterLodgeInventory)>,
) -> Option<CellCoord> {
    let Ok((_, building, owner, growth)) = tree_plots.get(tree_plot) else {
        return None;
    };
    if building.kind != BuildingKind::TreePlot || !growth.is_mature() {
        return None;
    }
    let Ok((lodge, inventory)) = lodges.get(owner.lodge()) else {
        return None;
    };
    (lodge.kind == BuildingKind::ForesterLodge && inventory.has_wood_capacity())
        .then_some(building.footprint.origin())
}

pub fn system_seed_tree_plots(
    mut commands: Commands,
    mut npcs: Query<(
        Entity,
        &crate::components::NpcPosition,
        &AiSeedTreePlot,
        Option<&Forester>,
        Option<&crate::components::AiSearchForFood>,
        Option<&crate::components::AiGatherResource>,
        Option<&crate::components::AiConstructBuilding>,
        Option<&crate::farming::AiSeedField>,
        Option<&crate::farming::AiHarvestField>,
        Option<&AiCutTreePlot>,
        Option<&mut NpcSkills>,
    )>,
    mut tree_plots: Query<(&Building, &TreePlotOwner, &mut TreePlotGrowth)>,
    lodges: Query<(&Building, &ForesterLodgeInventory)>,
) {
    for (
        npc,
        position,
        seed,
        forester,
        search,
        gather,
        construction,
        field_seed,
        field_harvest,
        cut,
        skills,
    ) in &mut npcs
    {
        if forester.is_none()
            || search.is_some()
            || gather.is_some()
            || construction.is_some()
            || field_seed.is_some()
            || field_harvest.is_some()
            || cut.is_some()
        {
            commands.entity(npc).remove::<AiSeedTreePlot>();
            continue;
        }
        let Ok((building, owner, mut growth)) = tree_plots.get_mut(seed.tree_plot()) else {
            commands.entity(npc).remove::<AiSeedTreePlot>();
            continue;
        };
        let Ok((lodge, _)) = lodges.get(owner.lodge()) else {
            commands.entity(npc).remove::<AiSeedTreePlot>();
            continue;
        };
        if building.kind != BuildingKind::TreePlot
            || lodge.kind != BuildingKind::ForesterLodge
            || !growth.is_seedable()
            || !building.footprint.contains(position.coord)
        {
            commands.entity(npc).remove::<AiSeedTreePlot>();
            continue;
        }
        if growth.advance_seeding_tick() {
            if let Some(mut skills) = skills {
                skills.add_xp(SkillKind::Lumberjack, 1);
            }
            commands.entity(npc).remove::<AiSeedTreePlot>();
        }
    }
}

pub fn system_cut_tree_plots(
    mut commands: Commands,
    mut npcs: Query<(
        Entity,
        &crate::components::NpcPosition,
        &mut AiCutTreePlot,
        Option<&Forester>,
        Option<&crate::components::AiSearchForFood>,
        Option<&crate::components::AiGatherResource>,
        Option<&crate::components::AiConstructBuilding>,
        Option<&crate::farming::AiSeedField>,
        Option<&crate::farming::AiHarvestField>,
        Option<&AiSeedTreePlot>,
        Option<&mut NpcSkills>,
    )>,
    mut tree_plots: Query<(&Building, &TreePlotOwner, &mut TreePlotGrowth)>,
    mut lodges: Query<(&Building, &mut ForesterLodgeInventory)>,
) {
    for (
        npc,
        position,
        mut cut,
        forester,
        search,
        gather,
        construction,
        field_seed,
        field_harvest,
        tree_seed,
        skills,
    ) in &mut npcs
    {
        if forester.is_none()
            || search.is_some()
            || gather.is_some()
            || construction.is_some()
            || field_seed.is_some()
            || field_harvest.is_some()
            || tree_seed.is_some()
        {
            commands.entity(npc).remove::<AiCutTreePlot>();
            continue;
        }
        let Ok((building, owner, mut growth)) = tree_plots.get_mut(cut.tree_plot()) else {
            commands.entity(npc).remove::<AiCutTreePlot>();
            continue;
        };
        let Ok((lodge, mut inventory)) = lodges.get_mut(owner.lodge()) else {
            commands.entity(npc).remove::<AiCutTreePlot>();
            continue;
        };
        if building.kind != BuildingKind::TreePlot
            || lodge.kind != BuildingKind::ForesterLodge
            || !growth.is_mature()
            || !inventory.has_wood_capacity()
            || !building.footprint.contains(position.coord)
        {
            commands.entity(npc).remove::<AiCutTreePlot>();
            continue;
        }
        cut.advance_tick();
        if cut.progress_ticks() < TREE_PLOT_CUTTING_TICKS {
            continue;
        }
        if inventory.add_wood(TREE_PLOT_WOOD_YIELD) {
            growth.reset_after_cut();
            if let Some(mut skills) = skills {
                skills.add_xp(SkillKind::Lumberjack, 1);
            }
        }
        commands.entity(npc).remove::<AiCutTreePlot>();
    }
}

fn tree_plot_batch_preview(
    world: &World,
    forester_lodge: Entity,
    coords: impl IntoIterator<Item = CellCoord>,
) -> Vec<TreePlotPlacementPreview> {
    let mut deduped = coords.into_iter().collect::<Vec<_>>();
    deduped.sort_by_key(|coord| (coord.y(), coord.x()));
    deduped.dedup();
    let lodge_footprint = match forester_lodge_footprint(world, forester_lodge) {
        Ok(footprint) => footprint,
        Err(error) => {
            return deduped
                .into_iter()
                .map(|coord| TreePlotPlacementPreview {
                    coord,
                    result: Err(error),
                })
                .collect();
        }
    };
    let remaining_capacity = MAX_TREE_PLOTS_PER_FORESTER_LODGE
        .saturating_sub(linked_tree_plot_count(world, forester_lodge));
    let mut valid_footprints = HashMap::new();
    let mut previews = Vec::with_capacity(deduped.len());
    for coord in deduped {
        match validate_tree_plot_footprint(world, coord) {
            Ok(footprint) => {
                valid_footprints.insert(coord, footprint);
                previews.push(TreePlotPlacementPreview {
                    coord,
                    result: Ok(footprint),
                });
            }
            Err(error) => previews.push(TreePlotPlacementPreview {
                coord,
                result: Err(error),
            }),
        }
    }
    let reachable = connected_batch_tree_plots(
        world,
        forester_lodge,
        lodge_footprint,
        valid_footprints.keys(),
        valid_footprints.len(),
    );
    let accepted = connected_batch_tree_plots(
        world,
        forester_lodge,
        lodge_footprint,
        valid_footprints.keys(),
        remaining_capacity,
    );
    for preview in &mut previews {
        if preview.result.is_err() {
            continue;
        }
        if !reachable.contains(&preview.coord) {
            preview.result = Err(TreePlotPlacementError::NotConnected);
        } else if !accepted.contains(&preview.coord) {
            preview.result = Err(TreePlotPlacementError::ForesterLodgeTreePlotLimitReached);
        }
    }
    previews
}

fn validate_tree_plot_footprint(
    world: &World,
    coord: CellCoord,
) -> Result<BuildingFootprint, TreePlotPlacementError> {
    validate_building_footprint_placement(world, BuildingKind::TreePlot, coord).map_err(Into::into)
}

fn forester_lodge_footprint(
    world: &World,
    lodge: Entity,
) -> Result<BuildingFootprint, TreePlotPlacementError> {
    if let Some(building) = world.get::<Building>(lodge) {
        return (building.kind == BuildingKind::ForesterLodge)
            .then_some(building.footprint)
            .ok_or(TreePlotPlacementError::OwnerNotForesterLodge);
    }
    if let Some(blueprint) = world.get::<BuildingBlueprint>(lodge) {
        return (blueprint.kind == BuildingKind::ForesterLodge)
            .then_some(blueprint.footprint)
            .ok_or(TreePlotPlacementError::OwnerNotForesterLodge);
    }
    Err(TreePlotPlacementError::OwnerMissing)
}

fn constructed_forester_lodge_inventory(
    world: &World,
    lodge: Entity,
) -> Option<ForesterLodgeInventory> {
    let building = world.get::<Building>(lodge)?;
    let inventory = world.get::<ForesterLodgeInventory>(lodge)?;
    (building.kind == BuildingKind::ForesterLodge).then_some(*inventory)
}

fn linked_tree_plot_count(world: &World, lodge: Entity) -> usize {
    linked_tree_plot_states(world)
        .into_iter()
        .filter(|(_, _, owner)| owner.lodge() == lodge)
        .count()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreePlotEntityState {
    Blueprint,
    Constructed,
}

fn linked_tree_plot_states(
    world: &World,
) -> Vec<(BuildingFootprint, TreePlotEntityState, TreePlotOwner)> {
    let mut plots = Vec::new();
    if let Some(mut query) = world.try_query::<(&BuildingBlueprint, &TreePlotOwner)>() {
        plots.extend(query.iter(world).filter_map(|(blueprint, owner)| {
            (blueprint.kind == BuildingKind::TreePlot).then_some((
                blueprint.footprint,
                TreePlotEntityState::Blueprint,
                *owner,
            ))
        }));
    }
    if let Some(mut query) = world.try_query::<(&Building, &TreePlotOwner)>() {
        plots.extend(query.iter(world).filter_map(|(building, owner)| {
            (building.kind == BuildingKind::TreePlot).then_some((
                building.footprint,
                TreePlotEntityState::Constructed,
                *owner,
            ))
        }));
    }
    plots
}

fn is_tree_plot_connected_to_network(
    world: &World,
    lodge: Entity,
    coord: CellCoord,
    lodge_footprint: BuildingFootprint,
    batch_plots: &HashSet<CellCoord>,
) -> bool {
    cardinal_neighbors(coord).into_iter().any(|neighbor| {
        lodge_footprint.contains(neighbor)
            || batch_plots.contains(&neighbor)
            || linked_tree_plot_states(world)
                .into_iter()
                .any(|(footprint, _, owner)| owner.lodge() == lodge && footprint.contains(neighbor))
    })
}

fn connected_batch_tree_plots<'a>(
    world: &World,
    lodge: Entity,
    lodge_footprint: BuildingFootprint,
    candidates: impl IntoIterator<Item = &'a CellCoord>,
    capacity: usize,
) -> HashSet<CellCoord> {
    connected_cells(candidates, capacity, |coord, connected| {
        is_tree_plot_connected_to_network(world, lodge, coord, lodge_footprint, connected)
    })
}

fn active_seed_tree_plots(world: &World) -> HashSet<Entity> {
    world
        .try_query::<&AiSeedTreePlot>()
        .map(|mut query| query.iter(world).map(|seed| seed.tree_plot()).collect())
        .unwrap_or_default()
}
