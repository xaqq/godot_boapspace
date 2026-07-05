use bevy_ecs::prelude::Resource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Resource)]
pub struct GameResources {
    wood: u32,
    stone: u32,
    food: u32,
    gold: u32,
}

impl GameResources {
    pub const STARTING_AMOUNT: u32 = 100;

    pub const fn new(wood: u32, stone: u32, food: u32, gold: u32) -> Self {
        Self {
            wood,
            stone,
            food,
            gold,
        }
    }

    pub const fn starting() -> Self {
        Self::new(
            Self::STARTING_AMOUNT,
            Self::STARTING_AMOUNT,
            Self::STARTING_AMOUNT,
            Self::STARTING_AMOUNT,
        )
    }

    pub fn get(&self, kind: ResourceKind) -> u32 {
        match kind {
            ResourceKind::Wood => self.wood,
            ResourceKind::Stone => self.stone,
            ResourceKind::Food => self.food,
            ResourceKind::Gold => self.gold,
        }
    }

    pub fn add(&mut self, kind: ResourceKind, amount: u32) -> bool {
        let current = self.get(kind);
        let Some(next) = current.checked_add(amount) else {
            return false;
        };

        self.set(kind, next);
        true
    }

    pub fn remove(&mut self, kind: ResourceKind, amount: u32) -> bool {
        let current = self.get(kind);
        if current >= amount {
            self.set(kind, current - amount);
            true
        } else {
            false
        }
    }

    fn set(&mut self, kind: ResourceKind, amount: u32) {
        match kind {
            ResourceKind::Wood => self.wood = amount,
            ResourceKind::Stone => self.stone = amount,
            ResourceKind::Food => self.food = amount,
            ResourceKind::Gold => self.gold = amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_zero() {
        let r = GameResources::default();
        assert_eq!(r.get(ResourceKind::Wood), 0);
        assert_eq!(r.get(ResourceKind::Stone), 0);
        assert_eq!(r.get(ResourceKind::Food), 0);
        assert_eq!(r.get(ResourceKind::Gold), 0);
    }

    #[test]
    fn test_starting_resources() {
        let r = GameResources::starting();
        for kind in ResourceKind::ALL {
            assert_eq!(r.get(kind), GameResources::STARTING_AMOUNT);
        }
    }

    #[test]
    fn test_add_resource() {
        let mut r = GameResources::default();
        assert!(r.add(ResourceKind::Wood, 10));
        assert_eq!(r.get(ResourceKind::Wood), 10);
        assert!(r.add(ResourceKind::Wood, 5));
        assert_eq!(r.get(ResourceKind::Wood), 15);
    }

    #[test]
    fn test_add_overflow_fails_without_mutating() {
        let mut r = GameResources::new(u32::MAX, 0, 0, 0);
        assert!(!r.add(ResourceKind::Wood, 1));
        assert_eq!(r.get(ResourceKind::Wood), u32::MAX);
    }

    #[test]
    fn test_remove_sufficient() {
        let mut r = GameResources::default();
        assert!(r.add(ResourceKind::Stone, 20));
        assert!(r.remove(ResourceKind::Stone, 10));
        assert_eq!(r.get(ResourceKind::Stone), 10);
    }

    #[test]
    fn test_remove_insufficient_fails() {
        let mut r = GameResources::default();
        assert!(r.add(ResourceKind::Gold, 5));
        assert!(!r.remove(ResourceKind::Gold, 10));
        assert_eq!(r.get(ResourceKind::Gold), 5);
    }

    #[test]
    fn test_remove_exact() {
        let mut r = GameResources::default();
        assert!(r.add(ResourceKind::Food, 10));
        assert!(r.remove(ResourceKind::Food, 10));
        assert_eq!(r.get(ResourceKind::Food), 0);
    }

    #[test]
    fn test_get_matches() {
        let r = GameResources::new(1, 2, 3, 4);
        assert_eq!(r.get(ResourceKind::Wood), 1);
        assert_eq!(r.get(ResourceKind::Stone), 2);
        assert_eq!(r.get(ResourceKind::Food), 3);
        assert_eq!(r.get(ResourceKind::Gold), 4);
    }
}
