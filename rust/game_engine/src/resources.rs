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

    pub fn set(&mut self, kind: ResourceKind, amount: u32) {
        self.amounts[kind as usize] = amount;
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
