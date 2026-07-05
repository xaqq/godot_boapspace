use super::resource_quantity::ResourceQuantity;
use crate::world::game_world::GameWorld;
use game_engine::resources::{GameResources, ResourceKind};
use godot::classes::{HBoxContainer, IHBoxContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = HBoxContainer)]
pub(crate) struct ResourceHeader {
    #[export]
    wood_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    stone_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    food_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    gold_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<HBoxContainer>,
}

#[godot_api]
impl IHBoxContainer for ResourceHeader {
    fn init(base: Base<HBoxContainer>) -> Self {
        Self {
            wood_quantity: OnEditor::default(),
            stone_quantity: OnEditor::default(),
            food_quantity: OnEditor::default(),
            gold_quantity: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let game_world = self.game_world.clone();
        let wood_quantity = self.wood_quantity.clone();
        let stone_quantity = self.stone_quantity.clone();
        let food_quantity = self.food_quantity.clone();
        let gold_quantity = self.gold_quantity.clone();

        let amounts = game_world.bind().with_rendered_surface_world(|world| {
            let resources = world.resource::<GameResources>();
            ResourceKind::ALL.map(|kind| resources.get(kind))
        });

        self.update_quantity(wood_quantity, amounts[resource_index(ResourceKind::Wood)]);
        self.update_quantity(stone_quantity, amounts[resource_index(ResourceKind::Stone)]);
        self.update_quantity(food_quantity, amounts[resource_index(ResourceKind::Food)]);
        self.update_quantity(gold_quantity, amounts[resource_index(ResourceKind::Gold)]);
    }
}

impl ResourceHeader {
    fn update_quantity(&self, mut quantity: Gd<ResourceQuantity>, amount: u32) {
        quantity.bind_mut().set_amount(amount);
    }
}

fn resource_index(kind: ResourceKind) -> usize {
    kind as usize
}
