use bevy_ecs::prelude::{Resource, World};
use godot::prelude::{Export, GodotConvert, Var};

use crate::buildings::{BuildingBlueprint, ConstructionProgress, WarehouseInventory};
use crate::components::NpcInventory;
use crate::farming::FarmInventory;
use crate::forestry::ForesterLodgeInventory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, GodotConvert, Var, Export)]
#[godot(via = i64)]
pub enum ResourceKind {
    Wood = 0,
    Stone = 1,
    Food = 2,
    Gold = 3,
}

impl ResourceKind {
    pub const ALL: [Self; 4] = [Self::Wood, Self::Stone, Self::Food, Self::Gold];

    pub const fn label(self) -> &'static str {
        match self {
            ResourceKind::Wood => "Wood",
            ResourceKind::Stone => "Stone",
            ResourceKind::Food => "Food",
            ResourceKind::Gold => "Gold",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            ResourceKind::Wood => {
                "Flexible timber used for basic construction, repairs, and early infrastructure."
            }
            ResourceKind::Stone => {
                "Durable building material for stronger structures and long-lived foundations."
            }
            ResourceKind::Food => "Essential supplies that keep colonists fed and productive.",
            ResourceKind::Gold => "Valuable currency used for advanced construction and trade.",
        }
    }
}

/// Overflow-safe quantities used by surface-wide resource queries and history.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceTotals {
    amounts: [u64; ResourceKind::ALL.len()],
}

impl ResourceTotals {
    pub const fn zero() -> Self {
        Self {
            amounts: [0; ResourceKind::ALL.len()],
        }
    }

    pub const fn get(self, kind: ResourceKind) -> u64 {
        self.amounts[kind as usize]
    }

    fn add_amounts(&mut self, amounts: ResourceAmounts) {
        for kind in ResourceKind::ALL {
            self.amounts[kind as usize] += u64::from(amounts.get(kind));
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceOverview {
    usable: ResourceTotals,
    committed: ResourceTotals,
}

impl ResourceOverview {
    pub const fn usable(self) -> ResourceTotals {
        self.usable
    }

    pub const fn committed(self) -> ResourceTotals {
        self.committed
    }
}

/// Returns live, surface-local resource totals. Natural resource nodes have no
/// eligible inventory component and are therefore intentionally excluded.
pub fn resource_overview(world: &World) -> ResourceOverview {
    let mut overview = ResourceOverview::default();

    for entity in world.iter_entities() {
        if let Some(inventory) = entity.get::<NpcInventory>() {
            overview.usable.add_amounts(inventory.contents());
        }
        if let Some(inventory) = entity.get::<WarehouseInventory>() {
            overview.usable.add_amounts(inventory.contents());
        }
        if let Some(inventory) = entity.get::<FarmInventory>() {
            overview.usable.add_amounts(inventory.contents());
        }
        if let Some(inventory) = entity.get::<ForesterLodgeInventory>() {
            overview.usable.add_amounts(inventory.contents());
        }

        if let (Some(blueprint), Some(progress)) = (
            entity.get::<BuildingBlueprint>(),
            entity.get::<ConstructionProgress>(),
        ) {
            let cost = blueprint.kind.definition().construction_cost();
            if !progress.is_complete(cost) {
                overview.committed.add_amounts(progress.deposited());
            }
        }
    }

    overview
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyResourceSample {
    day: u64,
    usable: ResourceTotals,
}

impl DailyResourceSample {
    pub const fn new(day: u64, usable: ResourceTotals) -> Self {
        Self { day, usable }
    }

    pub const fn day(self) -> u64 {
        self.day
    }

    pub const fn usable(self) -> ResourceTotals {
        self.usable
    }

    pub const fn quantity(self, kind: ResourceKind) -> u64 {
        self.usable.get(kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct ResourceHistory {
    samples: Vec<DailyResourceSample>,
}

impl ResourceHistory {
    pub fn new(initial_day: u64, initial_usable: ResourceTotals) -> Self {
        Self {
            samples: vec![DailyResourceSample::new(initial_day, initial_usable)],
        }
    }

    pub fn samples(&self) -> &[DailyResourceSample] {
        &self.samples
    }

    /// Records at most one immutable sample per day. Simulation time is
    /// monotonic, so older days are ignored as well as duplicates.
    pub fn record_day(&mut self, day: u64, usable: ResourceTotals) -> bool {
        if self
            .samples
            .last()
            .is_some_and(|sample| sample.day() >= day)
        {
            return false;
        }
        self.samples.push(DailyResourceSample::new(day, usable));
        true
    }

    pub fn sample_on(&self, day: u64) -> Option<DailyResourceSample> {
        self.samples
            .binary_search_by_key(&day, |sample| sample.day())
            .ok()
            .map(|index| self.samples[index])
    }

    pub fn change_since(
        &self,
        current_day: u64,
        lookback_days: u64,
        kind: ResourceKind,
        current_quantity: u64,
    ) -> Option<i128> {
        let target_day = current_day.checked_sub(lookback_days)?;
        let baseline = self.sample_on(target_day)?.quantity(kind);
        Some(i128::from(current_quantity) - i128::from(baseline))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceAmounts {
    amounts: [u32; ResourceKind::ALL.len()],
}

impl ResourceAmounts {
    pub const fn new(wood: u32, stone: u32, food: u32, gold: u32) -> Self {
        Self {
            amounts: [wood, stone, food, gold],
        }
    }

    pub const fn zero() -> Self {
        Self::new(0, 0, 0, 0)
    }

    pub const fn get(self, kind: ResourceKind) -> u32 {
        self.amounts[kind as usize]
    }

    pub const fn total(self) -> u32 {
        self.amounts[0]
            .saturating_add(self.amounts[1])
            .saturating_add(self.amounts[2])
            .saturating_add(self.amounts[3])
    }

    pub fn set(&mut self, kind: ResourceKind, amount: u32) {
        self.amounts[kind as usize] = amount;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceInventory {
    contents: ResourceAmounts,
    max_size: u32,
}

impl ResourceInventory {
    pub const fn empty(max_size: u32) -> Self {
        Self::new(ResourceAmounts::zero(), max_size)
    }

    pub const fn new(contents: ResourceAmounts, max_size: u32) -> Self {
        assert!(contents.total() <= max_size);
        Self { contents, max_size }
    }

    pub const fn contents(self) -> ResourceAmounts {
        self.contents
    }

    pub const fn max_size(self) -> u32 {
        self.max_size
    }

    pub const fn used_size(self) -> u32 {
        self.contents.total()
    }

    pub const fn free_size(self) -> u32 {
        self.max_size.saturating_sub(self.used_size())
    }

    pub fn consume(&mut self, kind: ResourceKind, amount: u32) -> bool {
        let available = self.contents.get(kind);
        if available < amount {
            return false;
        }

        self.contents.set(kind, available - amount);
        true
    }

    pub fn add(&mut self, kind: ResourceKind, amount: u32) -> bool {
        let available = self.contents.get(kind);
        let Some(new_amount) = available.checked_add(amount) else {
            return false;
        };
        if amount > self.free_size() {
            return false;
        }

        self.contents.set(kind, new_amount);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{
        BuildingBlueprint, BuildingFootprint, BuildingKind, ConstructionProgress,
        WarehouseInventory,
    };
    use crate::components::{NpcInventory, ResourceNode};
    use crate::farming::FarmInventory;
    use crate::forestry::ForesterLodgeInventory;
    use godot::prelude::{FromGodot, ToGodot};

    fn totals(amounts: ResourceAmounts) -> ResourceTotals {
        let mut totals = ResourceTotals::zero();
        totals.add_amounts(amounts);
        totals
    }

    #[test]
    fn resource_overview_sums_every_owned_inventory_and_excludes_natural_nodes() {
        let mut world = World::new();
        world.spawn(NpcInventory::new(ResourceAmounts::new(1, 2, 3, 4)));

        let mut warehouse = WarehouseInventory::empty();
        for (kind, amount) in [
            (ResourceKind::Wood, 10),
            (ResourceKind::Stone, 11),
            (ResourceKind::Food, 12),
            (ResourceKind::Gold, 13),
        ] {
            assert!(warehouse.add(kind, amount));
        }
        world.spawn(warehouse);

        let mut farm = FarmInventory::empty();
        assert!(farm.add_food(14));
        world.spawn(farm);

        let mut lodge = ForesterLodgeInventory::empty();
        assert!(lodge.add_wood(15));
        world.spawn(lodge);

        world.spawn(ResourceNode {
            kind: ResourceKind::Gold,
            quantity: u32::MAX,
        });

        let overview = resource_overview(&world);
        assert_eq!(overview.usable().get(ResourceKind::Wood), 26);
        assert_eq!(overview.usable().get(ResourceKind::Stone), 13);
        assert_eq!(overview.usable().get(ResourceKind::Food), 29);
        assert_eq!(overview.usable().get(ResourceKind::Gold), 17);
        assert_eq!(overview.committed(), ResourceTotals::zero());
    }

    #[test]
    fn resource_overview_counts_each_inventory_component_once() {
        let mut world = World::new();
        let mut warehouse = WarehouseInventory::empty();
        assert!(warehouse.add(ResourceKind::Wood, 7));
        world.spawn((
            NpcInventory::new(ResourceAmounts::new(5, 0, 0, 0)),
            warehouse,
        ));

        assert_eq!(
            resource_overview(&world).usable().get(ResourceKind::Wood),
            12
        );
    }

    #[test]
    fn resource_overview_only_commits_deposits_on_incomplete_blueprints() {
        let mut world = World::new();
        world.spawn((
            BuildingBlueprint {
                kind: BuildingKind::TownHall,
                footprint: BuildingFootprint::new(crate::grid::CellCoord::new(0, 0), 3, 3),
            },
            ConstructionProgress::new(ResourceAmounts::new(5, 6, 0, 1)),
        ));
        world.spawn((
            BuildingBlueprint {
                kind: BuildingKind::Warehouse,
                footprint: BuildingFootprint::new(crate::grid::CellCoord::new(4, 0), 2, 2),
            },
            ConstructionProgress::new(BuildingKind::Warehouse.definition().construction_cost()),
        ));
        world.spawn(ConstructionProgress::new(ResourceAmounts::new(9, 9, 9, 9)));

        let committed = resource_overview(&world).committed();
        assert_eq!(committed.get(ResourceKind::Wood), 5);
        assert_eq!(committed.get(ResourceKind::Stone), 6);
        assert_eq!(committed.get(ResourceKind::Food), 0);
        assert_eq!(committed.get(ResourceKind::Gold), 1);
    }

    #[test]
    fn resource_totals_accumulate_beyond_u32_range() {
        let mut totals = ResourceTotals::zero();
        totals.add_amounts(ResourceAmounts::new(u32::MAX, 0, 0, 0));
        totals.add_amounts(ResourceAmounts::new(1, 0, 0, 0));

        assert_eq!(totals.get(ResourceKind::Wood), u64::from(u32::MAX) + 1);
    }

    #[test]
    fn resource_history_keeps_samples_immutable_and_ordered() {
        let initial = totals(ResourceAmounts::new(1, 2, 3, 4));
        let mut history = ResourceHistory::new(10, initial);

        assert!(!history.record_day(10, totals(ResourceAmounts::new(9, 9, 9, 9))));
        assert!(!history.record_day(9, totals(ResourceAmounts::zero())));
        assert!(history.record_day(11, totals(ResourceAmounts::new(5, 6, 7, 8))));

        assert_eq!(history.samples().len(), 2);
        assert_eq!(history.sample_on(10).unwrap().usable(), initial);
        assert_eq!(
            history.sample_on(11).unwrap().quantity(ResourceKind::Gold),
            8
        );
    }

    #[test]
    fn resource_history_changes_require_an_exact_baseline_day() {
        let mut history = ResourceHistory::new(10, totals(ResourceAmounts::new(5, 8, 0, 0)));
        assert!(history.record_day(11, totals(ResourceAmounts::new(6, 3, 0, 0))));
        assert!(history.record_day(17, totals(ResourceAmounts::new(20, 20, 0, 0))));

        assert_eq!(history.change_since(17, 7, ResourceKind::Wood, 12), Some(7));
        assert_eq!(
            history.change_since(17, 7, ResourceKind::Stone, 2),
            Some(-6)
        );
        assert_eq!(history.change_since(11, 1, ResourceKind::Food, 0), Some(0));
        assert_eq!(history.change_since(17, 1, ResourceKind::Wood, 12), None);
        assert_eq!(history.change_since(3, 7, ResourceKind::Wood, 12), None);
    }

    #[test]
    fn resource_amounts_default_to_zero() {
        let amounts = ResourceAmounts::default();
        for kind in ResourceKind::ALL {
            assert_eq!(amounts.get(kind), 0);
        }
    }

    #[test]
    fn resource_amounts_set_only_changes_selected_kind() {
        let mut amounts = ResourceAmounts::zero();
        amounts.set(ResourceKind::Food, 12);

        assert_eq!(amounts.get(ResourceKind::Wood), 0);
        assert_eq!(amounts.get(ResourceKind::Stone), 0);
        assert_eq!(amounts.get(ResourceKind::Food), 12);
        assert_eq!(amounts.get(ResourceKind::Gold), 0);
    }

    #[test]
    fn resource_amounts_get_matches_constructor_values() {
        let amounts = ResourceAmounts::new(1, 2, 3, 4);
        assert_eq!(amounts.get(ResourceKind::Wood), 1);
        assert_eq!(amounts.get(ResourceKind::Stone), 2);
        assert_eq!(amounts.get(ResourceKind::Food), 3);
        assert_eq!(amounts.get(ResourceKind::Gold), 4);
    }

    #[test]
    fn resource_amounts_total_sums_all_resources() {
        assert_eq!(ResourceAmounts::new(1, 2, 3, 4).total(), 10);
        assert_eq!(ResourceAmounts::new(u32::MAX, 1, 0, 0).total(), u32::MAX);
    }

    #[test]
    fn resource_inventory_tracks_used_and_free_size() {
        let inventory = ResourceInventory::new(ResourceAmounts::new(1, 2, 3, 4), 20);

        assert_eq!(inventory.contents(), ResourceAmounts::new(1, 2, 3, 4));
        assert_eq!(inventory.max_size(), 20);
        assert_eq!(inventory.used_size(), 10);
        assert_eq!(inventory.free_size(), 10);
    }

    #[test]
    fn resource_inventory_adds_within_capacity() {
        let mut inventory = ResourceInventory::new(ResourceAmounts::new(1, 0, 0, 0), 3);

        assert!(inventory.add(ResourceKind::Food, 2));
        assert_eq!(inventory.contents(), ResourceAmounts::new(1, 0, 2, 0));
        assert_eq!(inventory.used_size(), 3);
        assert_eq!(inventory.free_size(), 0);
    }

    #[test]
    fn resource_inventory_add_fails_without_mutation_when_over_capacity() {
        let mut inventory = ResourceInventory::new(ResourceAmounts::new(1, 0, 0, 0), 2);

        assert!(!inventory.add(ResourceKind::Food, 2));
        assert_eq!(inventory.contents(), ResourceAmounts::new(1, 0, 0, 0));
    }

    #[test]
    fn resource_inventory_add_fails_without_mutation_when_resource_overflows() {
        let mut inventory =
            ResourceInventory::new(ResourceAmounts::new(u32::MAX, 0, 0, 0), u32::MAX);

        assert!(!inventory.add(ResourceKind::Wood, 1));
        assert_eq!(
            inventory.contents(),
            ResourceAmounts::new(u32::MAX, 0, 0, 0)
        );
    }

    #[test]
    fn resource_inventory_consume_is_all_or_nothing() {
        let mut inventory = ResourceInventory::new(ResourceAmounts::new(0, 0, 2, 0), 10);

        assert!(!inventory.consume(ResourceKind::Food, 3));
        assert_eq!(inventory.contents(), ResourceAmounts::new(0, 0, 2, 0));
        assert!(inventory.consume(ResourceKind::Food, 2));
        assert_eq!(inventory.contents(), ResourceAmounts::zero());
    }

    #[test]
    fn resource_kind_round_trips_through_godot_value() {
        for kind in ResourceKind::ALL {
            let value: i64 = kind.to_godot();
            let round_tripped =
                ResourceKind::try_from_godot(value).expect("resource kind should round-trip");

            assert_eq!(round_tripped, kind);
        }
    }

    #[test]
    fn resource_kind_metadata_is_present() {
        for kind in ResourceKind::ALL {
            assert!(!kind.label().is_empty());
            assert!(!kind.description().is_empty());
        }
    }

    #[test]
    fn resource_kind_descriptions_match_expected_text() {
        assert_eq!(
            ResourceKind::Wood.description(),
            "Flexible timber used for basic construction, repairs, and early infrastructure."
        );
        assert_eq!(
            ResourceKind::Stone.description(),
            "Durable building material for stronger structures and long-lived foundations."
        );
        assert_eq!(
            ResourceKind::Food.description(),
            "Essential supplies that keep colonists fed and productive."
        );
        assert_eq!(
            ResourceKind::Gold.description(),
            "Valuable currency used for advanced construction and trade."
        );
    }
}
