use bevy_ecs::prelude::Resource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Wood = 0,
    Stone = 1,
    Food = 2,
    Gold = 3,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Resource)]
pub struct GameResources {
    pub wood: u32,
    pub stone: u32,
    pub food: u32,
    pub gold: u32,
}

impl GameResources {
    pub fn get(&self, kind: ResourceKind) -> u32 {
        match kind {
            ResourceKind::Wood => self.wood,
            ResourceKind::Stone => self.stone,
            ResourceKind::Food => self.food,
            ResourceKind::Gold => self.gold,
        }
    }

    pub fn add(&mut self, kind: ResourceKind, amount: u32) {
        match kind {
            ResourceKind::Wood => self.wood += amount,
            ResourceKind::Stone => self.stone += amount,
            ResourceKind::Food => self.food += amount,
            ResourceKind::Gold => self.gold += amount,
        }
    }

    pub fn remove(&mut self, kind: ResourceKind, amount: u32) -> bool {
        let current = self.get(kind);
        if current >= amount {
            match kind {
                ResourceKind::Wood => self.wood -= amount,
                ResourceKind::Stone => self.stone -= amount,
                ResourceKind::Food => self.food -= amount,
                ResourceKind::Gold => self.gold -= amount,
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_zero() {
        let r = GameResources::default();
        assert_eq!(r.wood, 0);
        assert_eq!(r.stone, 0);
        assert_eq!(r.food, 0);
        assert_eq!(r.gold, 0);
    }

    #[test]
    fn test_add_resource() {
        let mut r = GameResources::default();
        r.add(ResourceKind::Wood, 10);
        assert_eq!(r.wood, 10);
        r.add(ResourceKind::Wood, 5);
        assert_eq!(r.wood, 15);
    }

    #[test]
    fn test_remove_sufficient() {
        let mut r = GameResources::default();
        r.add(ResourceKind::Stone, 20);
        assert!(r.remove(ResourceKind::Stone, 10));
        assert_eq!(r.stone, 10);
    }

    #[test]
    fn test_remove_insufficient_fails() {
        let mut r = GameResources::default();
        r.add(ResourceKind::Gold, 5);
        assert!(!r.remove(ResourceKind::Gold, 10));
        assert_eq!(r.gold, 5);
    }

    #[test]
    fn test_remove_exact() {
        let mut r = GameResources::default();
        r.add(ResourceKind::Food, 10);
        assert!(r.remove(ResourceKind::Food, 10));
        assert_eq!(r.food, 0);
    }

    #[test]
    fn test_get_matches() {
        let mut r = GameResources::default();
        r.wood = 1;
        r.stone = 2;
        r.food = 3;
        r.gold = 4;
        assert_eq!(r.get(ResourceKind::Wood), 1);
        assert_eq!(r.get(ResourceKind::Stone), 2);
        assert_eq!(r.get(ResourceKind::Food), 3);
        assert_eq!(r.get(ResourceKind::Gold), 4);
    }
}
