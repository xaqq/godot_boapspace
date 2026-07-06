use crate::ai::RESOURCE_GATHER_TICKS_PER_UNIT;
use crate::buildings::{
    validate_building_footprint_placement, Building, BuildingBlueprint, BuildingFootprint,
    BuildingKind, BuildingPlacementError,
};
use crate::grid::CellCoord;
use crate::resources::{ResourceAmounts, ResourceInventory, ResourceKind};
use crate::skills::{NpcSkills, SkillKind};
use crate::time::SIMULATION_TICKS_PER_DAY;
use bevy_ecs::prelude::*;
use std::collections::{HashMap, HashSet};

pub const MAX_FIELDS_PER_FARM: usize = 200;
pub const FARM_INVENTORY_MAX_FOOD: u32 = 200;
pub const FIELD_SEEDING_TICKS: u32 = SIMULATION_TICKS_PER_DAY;
pub const FIELD_GROWTH_TICKS: u32 = SIMULATION_TICKS_PER_DAY * 365;
pub const FIELD_HARVEST_TICKS: u32 = RESOURCE_GATHER_TICKS_PER_UNIT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Farmer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct FarmInventory {
    inventory: ResourceInventory,
}

impl FarmInventory {
    pub const fn empty() -> Self {
        Self {
            inventory: ResourceInventory::empty(FARM_INVENTORY_MAX_FOOD),
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

    pub const fn food(self) -> u32 {
        self.inventory.contents().get(ResourceKind::Food)
    }

    pub const fn has_food_capacity(self) -> bool {
        self.free_size() > 0
    }

    pub fn add_food(&mut self, amount: u32) -> bool {
        self.inventory.add(ResourceKind::Food, amount)
    }
}

impl Default for FarmInventory {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct FieldOwner {
    farm: Entity,
}

impl FieldOwner {
    pub const fn new(farm: Entity) -> Self {
        Self { farm }
    }

    pub const fn farm(self) -> Entity {
        self.farm
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct FieldCrop {
    seeding_progress_ticks: u32,
    growth_ticks: Option<u32>,
}

impl FieldCrop {
    pub const fn seedable() -> Self {
        Self {
            seeding_progress_ticks: 0,
            growth_ticks: None,
        }
    }

    pub const fn with_seeding_progress(seeding_progress_ticks: u32) -> Self {
        Self {
            seeding_progress_ticks,
            growth_ticks: None,
        }
    }

    pub const fn growing(growth_ticks: u32) -> Self {
        Self {
            seeding_progress_ticks: FIELD_SEEDING_TICKS,
            growth_ticks: Some(growth_ticks),
        }
    }

    pub const fn seeding_progress_ticks(self) -> u32 {
        self.seeding_progress_ticks
    }

    pub const fn growth_ticks(self) -> Option<u32> {
        self.growth_ticks
    }

    pub const fn is_seedable(self) -> bool {
        self.growth_ticks.is_none() && self.seeding_progress_ticks < FIELD_SEEDING_TICKS
    }

    pub const fn is_grown(self) -> bool {
        match self.growth_ticks {
            Some(ticks) => ticks >= FIELD_GROWTH_TICKS,
            None => false,
        }
    }

    pub fn advance_seeding_tick(&mut self) -> bool {
        if self.growth_ticks.is_some() || self.seeding_progress_ticks >= FIELD_SEEDING_TICKS {
            return false;
        }

        self.seeding_progress_ticks = self.seeding_progress_ticks.saturating_add(1);
        if self.seeding_progress_ticks >= FIELD_SEEDING_TICKS {
            self.seeding_progress_ticks = FIELD_SEEDING_TICKS;
            self.growth_ticks = Some(0);
            true
        } else {
            false
        }
    }

    pub fn advance_growth_tick(&mut self) {
        if let Some(ticks) = &mut self.growth_ticks {
            if *ticks < FIELD_GROWTH_TICKS {
                *ticks = ticks.saturating_add(1).min(FIELD_GROWTH_TICKS);
            }
        }
    }

    pub fn reset_after_harvest(&mut self) {
        *self = Self::seedable();
    }

    pub fn state(self, farm_active: bool, actively_seeding: bool) -> FieldCropState {
        if !farm_active {
            return FieldCropState::Inactive;
        }
        if actively_seeding && self.is_seedable() {
            return FieldCropState::Seeding;
        }
        match self.growth_ticks {
            None => FieldCropState::Seedable,
            Some(ticks) if ticks >= FIELD_GROWTH_TICKS => FieldCropState::Grown,
            Some(ticks) if ticks >= FIELD_GROWTH_TICKS / 2 => FieldCropState::GrowingStep2,
            Some(_) => FieldCropState::GrowingStep1,
        }
    }
}

impl Default for FieldCrop {
    fn default() -> Self {
        Self::seedable()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldCropState {
    Inactive,
    Seedable,
    Seeding,
    GrowingStep1,
    GrowingStep2,
    Grown,
}

impl FieldCropState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::Seedable => "Seedable",
            Self::Seeding => "Seeding",
            Self::GrowingStep1 => "Growing Step 1",
            Self::GrowingStep2 => "Growing Step 2",
            Self::Grown => "Grown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPlacementError {
    OutOfBounds,
    OverlapsBuilding,
    OwnerMissing,
    OwnerNotFarm,
    NotConnected,
    FarmFieldLimitReached,
}

impl From<BuildingPlacementError> for FieldPlacementError {
    fn from(value: BuildingPlacementError) -> Self {
        match value {
            BuildingPlacementError::OutOfBounds => Self::OutOfBounds,
            BuildingPlacementError::OverlapsBuilding => Self::OverlapsBuilding,
            BuildingPlacementError::FieldRequiresFarm => Self::OwnerMissing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldPlacementPreview {
    pub coord: CellCoord,
    pub result: Result<BuildingFootprint, FieldPlacementError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedField {
    pub coord: CellCoord,
    pub entity: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedFieldPlacement {
    pub coord: CellCoord,
    pub error: FieldPlacementError,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FieldPlacementBatchResult {
    pub placed: Vec<PlacedField>,
    pub rejected: Vec<RejectedFieldPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct SeedField {
    field: Entity,
}

impl SeedField {
    pub const fn new(field: Entity) -> Self {
        Self { field }
    }

    pub const fn field(self) -> Entity {
        self.field
    }

    pub const fn label() -> &'static str {
        "SeedField"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct HarvestField {
    field: Entity,
}

impl HarvestField {
    pub const fn new(field: Entity) -> Self {
        Self { field }
    }

    pub const fn field(self) -> Entity {
        self.field
    }

    pub const fn label() -> &'static str {
        "HarvestField"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiSeedField {
    field: Entity,
}

impl AiSeedField {
    pub const fn new(field: Entity) -> Self {
        Self { field }
    }

    pub const fn field(self) -> Entity {
        self.field
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct AiHarvestField {
    field: Entity,
    progress_ticks: u32,
}

impl AiHarvestField {
    pub const fn new(field: Entity) -> Self {
        Self {
            field,
            progress_ticks: 0,
        }
    }

    pub const fn field(self) -> Entity {
        self.field
    }

    pub const fn progress_ticks(self) -> u32 {
        self.progress_ticks
    }

    pub fn advance_tick(&mut self) {
        self.progress_ticks = self.progress_ticks.saturating_add(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Bundle)]
struct SeedFieldTaskBundle {
    task: crate::tasks::Task,
    seed: SeedField,
}

impl SeedFieldTaskBundle {
    const fn new(field: Entity) -> Self {
        Self {
            task: crate::tasks::Task,
            seed: SeedField::new(field),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Bundle)]
struct HarvestFieldTaskBundle {
    task: crate::tasks::Task,
    harvest: HarvestField,
}

impl HarvestFieldTaskBundle {
    const fn new(field: Entity) -> Self {
        Self {
            task: crate::tasks::Task,
            harvest: HarvestField::new(field),
        }
    }
}

pub fn validate_field_blueprint_placement(
    world: &World,
    farm: Entity,
    coord: CellCoord,
) -> Result<BuildingFootprint, FieldPlacementError> {
    let farm_footprint = farm_footprint(world, farm)?;
    let existing_field_count = linked_field_count(world, farm);
    if existing_field_count >= MAX_FIELDS_PER_FARM {
        return Err(FieldPlacementError::FarmFieldLimitReached);
    }

    let footprint = validate_field_footprint(world, coord)?;
    if !is_field_connected_to_network(world, farm, coord, farm_footprint, &HashSet::new()) {
        return Err(FieldPlacementError::NotConnected);
    }

    Ok(footprint)
}

pub fn validate_field_blueprint_placement_batch(
    world: &World,
    farm: Entity,
    coords: impl IntoIterator<Item = CellCoord>,
) -> Vec<FieldPlacementPreview> {
    field_batch_preview(world, farm, coords)
}

pub fn place_field_blueprint(
    world: &mut World,
    farm: Entity,
    coord: CellCoord,
) -> Result<Entity, FieldPlacementError> {
    let footprint = validate_field_blueprint_placement(world, farm, coord)?;
    let entity = world
        .spawn((
            crate::buildings::BuildingBlueprintBundle::new(BuildingKind::Field, footprint),
            FieldOwner::new(farm),
        ))
        .id();

    Ok(entity)
}

pub fn place_field_blueprints(
    world: &mut World,
    farm: Entity,
    coords: impl IntoIterator<Item = CellCoord>,
) -> FieldPlacementBatchResult {
    let previews = field_batch_preview(world, farm, coords);
    let mut result = FieldPlacementBatchResult::default();

    for preview in previews {
        match preview.result {
            Ok(footprint) => {
                let entity = world
                    .spawn((
                        crate::buildings::BuildingBlueprintBundle::new(
                            BuildingKind::Field,
                            footprint,
                        ),
                        FieldOwner::new(farm),
                    ))
                    .id();
                result.placed.push(PlacedField {
                    coord: preview.coord,
                    entity,
                });
            }
            Err(error) => {
                result.rejected.push(RejectedFieldPlacement {
                    coord: preview.coord,
                    error,
                });
            }
        }
    }

    result
}

pub fn field_crop_state(world: &World, field: Entity) -> Option<FieldCropState> {
    let crop = *world.get::<FieldCrop>(field)?;
    let owner = *world.get::<FieldOwner>(field)?;
    let farm_active = constructed_farm_inventory(world, owner.farm()).is_some();
    let actively_seeding = active_seed_fields(world).contains(&field);
    Some(crop.state(farm_active, actively_seeding))
}

pub fn farm_field_counts(world: &World, farm: Entity) -> (usize, usize) {
    let mut linked = 0;
    let mut constructed = 0;
    for (_, state, owner) in linked_field_states(world) {
        if owner.farm() != farm {
            continue;
        }
        linked += 1;
        if state == FieldEntityState::Constructed {
            constructed += 1;
        }
    }
    (linked, constructed)
}

pub fn system_advance_field_growth(mut fields: Query<&mut FieldCrop>) {
    for mut crop in &mut fields {
        crop.advance_growth_tick();
    }
}

pub fn maintain_farming_tasks(
    mut commands: Commands,
    fields: Query<(Entity, &Building, &FieldOwner, &FieldCrop)>,
    farms: Query<(&Building, &FarmInventory)>,
    seed_tasks: Query<(Entity, &SeedField)>,
    harvest_tasks: Query<(Entity, &HarvestField)>,
) {
    let mut seedable_fields = HashSet::new();
    let mut harvestable_fields = HashSet::new();

    for (field_entity, building, owner, crop) in &fields {
        if building.kind != BuildingKind::Field {
            continue;
        }
        let Ok((farm, inventory)) = farms.get(owner.farm()) else {
            continue;
        };
        if farm.kind != BuildingKind::Farm {
            continue;
        }

        if crop.is_seedable() {
            seedable_fields.insert(field_entity);
        }
        if crop.is_grown() && inventory.has_food_capacity() {
            harvestable_fields.insert(field_entity);
        }
    }

    let mut represented_seed_fields = HashSet::new();
    for (task_entity, task) in &seed_tasks {
        let field = task.field();
        if !seedable_fields.contains(&field) || !represented_seed_fields.insert(field) {
            commands.entity(task_entity).despawn();
        }
    }

    let mut represented_harvest_fields = HashSet::new();
    for (task_entity, task) in &harvest_tasks {
        let field = task.field();
        if !harvestable_fields.contains(&field) || !represented_harvest_fields.insert(field) {
            commands.entity(task_entity).despawn();
        }
    }

    for field in seedable_fields {
        if !represented_seed_fields.contains(&field) {
            commands.spawn(SeedFieldTaskBundle::new(field));
        }
    }

    for field in harvestable_fields {
        if !represented_harvest_fields.contains(&field) {
            commands.spawn(HarvestFieldTaskBundle::new(field));
        }
    }
}

pub fn field_seeding_is_actionable(
    field: Entity,
    fields: &Query<(Entity, &Building, &FieldOwner, &FieldCrop)>,
    farms: &Query<(&Building, &FarmInventory)>,
) -> Option<CellCoord> {
    let Ok((_, building, owner, crop)) = fields.get(field) else {
        return None;
    };
    if building.kind != BuildingKind::Field || !crop.is_seedable() {
        return None;
    }
    let Ok((farm, _)) = farms.get(owner.farm()) else {
        return None;
    };
    (farm.kind == BuildingKind::Farm).then_some(building.footprint.origin())
}

pub fn field_harvest_is_actionable(
    field: Entity,
    fields: &Query<(Entity, &Building, &FieldOwner, &FieldCrop)>,
    farms: &Query<(&Building, &FarmInventory)>,
) -> Option<CellCoord> {
    let Ok((_, building, owner, crop)) = fields.get(field) else {
        return None;
    };
    if building.kind != BuildingKind::Field || !crop.is_grown() {
        return None;
    }
    let Ok((farm, inventory)) = farms.get(owner.farm()) else {
        return None;
    };
    (farm.kind == BuildingKind::Farm && inventory.has_food_capacity())
        .then_some(building.footprint.origin())
}

pub fn system_seed_fields(
    mut commands: Commands,
    mut npcs: Query<(
        Entity,
        &crate::components::NpcPosition,
        &AiSeedField,
        Option<&Farmer>,
        Option<&crate::components::AiSearchForFood>,
        Option<&crate::components::AiGatherResource>,
        Option<&crate::components::AiConstructBuilding>,
        Option<&mut NpcSkills>,
    )>,
    mut fields: Query<(&Building, &FieldOwner, &mut FieldCrop)>,
    farms: Query<(&Building, &FarmInventory)>,
) {
    for (npc, position, seed, farmer, search, gather, construction, skills) in &mut npcs {
        if farmer.is_none() || search.is_some() || gather.is_some() || construction.is_some() {
            commands.entity(npc).remove::<AiSeedField>();
            continue;
        }

        let Ok((building, owner, mut crop)) = fields.get_mut(seed.field()) else {
            commands.entity(npc).remove::<AiSeedField>();
            continue;
        };
        let Ok((farm, _)) = farms.get(owner.farm()) else {
            commands.entity(npc).remove::<AiSeedField>();
            continue;
        };
        if building.kind != BuildingKind::Field
            || farm.kind != BuildingKind::Farm
            || !crop.is_seedable()
            || !building.footprint.contains(position.coord)
        {
            commands.entity(npc).remove::<AiSeedField>();
            continue;
        }

        if crop.advance_seeding_tick() {
            if let Some(mut skills) = skills {
                skills.add_xp(SkillKind::Farmer, 1);
            }
            commands.entity(npc).remove::<AiSeedField>();
        }
    }
}

pub fn system_harvest_fields(
    mut commands: Commands,
    mut npcs: Query<(
        Entity,
        &crate::components::NpcPosition,
        &mut AiHarvestField,
        Option<&Farmer>,
        Option<&crate::components::AiSearchForFood>,
        Option<&crate::components::AiGatherResource>,
        Option<&crate::components::AiConstructBuilding>,
        Option<&mut NpcSkills>,
    )>,
    mut fields: Query<(&Building, &FieldOwner, &mut FieldCrop)>,
    mut farms: Query<(&Building, &mut FarmInventory)>,
) {
    for (npc, position, mut harvest, farmer, search, gather, construction, skills) in &mut npcs {
        if farmer.is_none() || search.is_some() || gather.is_some() || construction.is_some() {
            commands.entity(npc).remove::<AiHarvestField>();
            continue;
        }

        let Ok((building, owner, mut crop)) = fields.get_mut(harvest.field()) else {
            commands.entity(npc).remove::<AiHarvestField>();
            continue;
        };
        let Ok((farm, mut inventory)) = farms.get_mut(owner.farm()) else {
            commands.entity(npc).remove::<AiHarvestField>();
            continue;
        };
        if building.kind != BuildingKind::Field
            || farm.kind != BuildingKind::Farm
            || !crop.is_grown()
            || !building.footprint.contains(position.coord)
        {
            commands.entity(npc).remove::<AiHarvestField>();
            continue;
        }

        harvest.advance_tick();
        if harvest.progress_ticks() < FIELD_HARVEST_TICKS {
            continue;
        }

        if inventory.add_food(1) {
            crop.reset_after_harvest();
            if let Some(mut skills) = skills {
                skills.add_xp(SkillKind::Farmer, 1);
            }
        }
        commands.entity(npc).remove::<AiHarvestField>();
    }
}

fn field_batch_preview(
    world: &World,
    farm: Entity,
    coords: impl IntoIterator<Item = CellCoord>,
) -> Vec<FieldPlacementPreview> {
    let mut deduped = coords.into_iter().collect::<Vec<_>>();
    deduped.sort_by_key(|coord| (coord.y(), coord.x()));
    deduped.dedup();

    let farm_footprint = match farm_footprint(world, farm) {
        Ok(footprint) => footprint,
        Err(error) => {
            return deduped
                .into_iter()
                .map(|coord| FieldPlacementPreview {
                    coord,
                    result: Err(error),
                })
                .collect();
        }
    };

    let existing_count = linked_field_count(world, farm);
    let remaining_capacity = MAX_FIELDS_PER_FARM.saturating_sub(existing_count);

    let mut valid_footprints = HashMap::new();
    let mut previews = Vec::with_capacity(deduped.len());
    for coord in deduped {
        match validate_field_footprint(world, coord) {
            Ok(footprint) => {
                valid_footprints.insert(coord, footprint);
                previews.push(FieldPlacementPreview {
                    coord,
                    result: Ok(footprint),
                });
            }
            Err(error) => previews.push(FieldPlacementPreview {
                coord,
                result: Err(error),
            }),
        }
    }

    let connected = connected_batch_fields(world, farm, farm_footprint, valid_footprints.keys());
    let mut accepted = 0usize;
    for preview in &mut previews {
        if preview.result.is_err() {
            continue;
        }
        if !connected.contains(&preview.coord) {
            preview.result = Err(FieldPlacementError::NotConnected);
            continue;
        }
        if accepted >= remaining_capacity {
            preview.result = Err(FieldPlacementError::FarmFieldLimitReached);
            continue;
        }
        accepted += 1;
    }

    previews
}

fn validate_field_footprint(
    world: &World,
    coord: CellCoord,
) -> Result<BuildingFootprint, FieldPlacementError> {
    validate_building_footprint_placement(world, BuildingKind::Field, coord).map_err(Into::into)
}

fn farm_footprint(world: &World, farm: Entity) -> Result<BuildingFootprint, FieldPlacementError> {
    if let Some(building) = world.get::<Building>(farm) {
        return (building.kind == BuildingKind::Farm)
            .then_some(building.footprint)
            .ok_or(FieldPlacementError::OwnerNotFarm);
    }
    if let Some(blueprint) = world.get::<BuildingBlueprint>(farm) {
        return (blueprint.kind == BuildingKind::Farm)
            .then_some(blueprint.footprint)
            .ok_or(FieldPlacementError::OwnerNotFarm);
    }

    Err(FieldPlacementError::OwnerMissing)
}

fn constructed_farm_inventory(world: &World, farm: Entity) -> Option<FarmInventory> {
    let building = world.get::<Building>(farm)?;
    let inventory = world.get::<FarmInventory>(farm)?;
    (building.kind == BuildingKind::Farm).then_some(*inventory)
}

fn linked_field_count(world: &World, farm: Entity) -> usize {
    linked_field_states(world)
        .into_iter()
        .filter(|(_, _, owner)| owner.farm() == farm)
        .count()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldEntityState {
    Blueprint,
    Constructed,
}

fn linked_field_states(world: &World) -> Vec<(BuildingFootprint, FieldEntityState, FieldOwner)> {
    let mut fields = Vec::new();

    if let Some(mut query) = world.try_query::<(&BuildingBlueprint, &FieldOwner)>() {
        fields.extend(query.iter(world).filter_map(|(blueprint, owner)| {
            (blueprint.kind == BuildingKind::Field).then_some((
                blueprint.footprint,
                FieldEntityState::Blueprint,
                *owner,
            ))
        }));
    }

    if let Some(mut query) = world.try_query::<(&Building, &FieldOwner)>() {
        fields.extend(query.iter(world).filter_map(|(building, owner)| {
            (building.kind == BuildingKind::Field).then_some((
                building.footprint,
                FieldEntityState::Constructed,
                *owner,
            ))
        }));
    }

    fields
}

fn is_field_connected_to_network(
    world: &World,
    farm: Entity,
    coord: CellCoord,
    farm_footprint: BuildingFootprint,
    batch_fields: &HashSet<CellCoord>,
) -> bool {
    cardinal_neighbors(coord).into_iter().any(|neighbor| {
        farm_footprint.contains(neighbor)
            || batch_fields.contains(&neighbor)
            || linked_field_states(world)
                .into_iter()
                .any(|(footprint, _, owner)| owner.farm() == farm && footprint.contains(neighbor))
    })
}

fn connected_batch_fields<'a>(
    world: &World,
    farm: Entity,
    farm_footprint: BuildingFootprint,
    candidates: impl IntoIterator<Item = &'a CellCoord>,
) -> HashSet<CellCoord> {
    let candidates = candidates.into_iter().copied().collect::<HashSet<_>>();
    let mut connected = HashSet::new();

    loop {
        let mut changed = false;
        for coord in &candidates {
            if connected.contains(coord) {
                continue;
            }
            if is_field_connected_to_network(world, farm, *coord, farm_footprint, &connected) {
                connected.insert(*coord);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    connected
}

fn cardinal_neighbors(coord: CellCoord) -> [CellCoord; 4] {
    [
        CellCoord::new(coord.x() + 1, coord.y()),
        CellCoord::new(coord.x() - 1, coord.y()),
        CellCoord::new(coord.x(), coord.y() + 1),
        CellCoord::new(coord.x(), coord.y() - 1),
    ]
}

fn active_seed_fields(world: &World) -> HashSet<Entity> {
    world
        .try_query::<&AiSeedField>()
        .map(|mut query| query.iter(world).map(|seed| seed.field()).collect())
        .unwrap_or_default()
}
