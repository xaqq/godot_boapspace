use godot::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceKind {
    Wood = 0,
    Stone = 1,
    Food = 2,
    Gold = 3,
}

#[derive(Debug, Clone, Default)]
pub struct ResourcesState {
    pub wood: u32,
    pub stone: u32,
    pub food: u32,
    pub gold: u32,
}

impl ResourcesState {
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

#[derive(GodotClass)]
#[class(base = Node)]
pub(crate) struct ResourceManager {
    state: ResourcesState,

    base: Base<Node>,
}

#[godot_api]
impl INode for ResourceManager {
    fn init(base: Base<Node>) -> Self {
        Self {
            state: ResourcesState::default(),
            base,
        }
    }
}

#[godot_api]
impl ResourceManager {
    #[signal]
    pub(crate) fn resources_changed();

    #[func]
    pub(crate) fn wood(&self) -> u32 {
        self.state.wood
    }

    #[func]
    pub(crate) fn stone(&self) -> u32 {
        self.state.stone
    }

    #[func]
    pub(crate) fn food(&self) -> u32 {
        self.state.food
    }

    #[func]
    pub(crate) fn gold(&self) -> u32 {
        self.state.gold
    }

    #[func]
    pub(crate) fn add_wood(&mut self, amount: u32) {
        self.state.add(ResourceKind::Wood, amount);
        self.signals().resources_changed().emit();
    }

    #[func]
    pub(crate) fn add_stone(&mut self, amount: u32) {
        self.state.add(ResourceKind::Stone, amount);
        self.signals().resources_changed().emit();
    }

    #[func]
    pub(crate) fn add_food(&mut self, amount: u32) {
        self.state.add(ResourceKind::Food, amount);
        self.signals().resources_changed().emit();
    }

    #[func]
    pub(crate) fn add_gold(&mut self, amount: u32) {
        self.state.add(ResourceKind::Gold, amount);
        self.signals().resources_changed().emit();
    }
}
