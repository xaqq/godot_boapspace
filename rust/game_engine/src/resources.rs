use godot::prelude::{Export, GodotConvert, Var};

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
    use godot::prelude::{FromGodot, ToGodot};

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
