use godot::classes::{Control, IControl, Label};
use godot::obj::OnEditor;
use godot::prelude::*;
use crate::game_world::GameWorld;

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct ResourceHeader {
    #[export]
    wood_label: OnEditor<Gd<Label>>,

    #[export]
    stone_label: OnEditor<Gd<Label>>,

    #[export]
    food_label: OnEditor<Gd<Label>>,

    #[export]
    gold_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    cached_wood: u32,
    cached_stone: u32,
    cached_food: u32,
    cached_gold: u32,

    base: Base<Control>,
}

#[godot_api]
impl IControl for ResourceHeader {
    fn init(base: Base<Control>) -> Self {
        Self {
            wood_label: OnEditor::default(),
            stone_label: OnEditor::default(),
            food_label: OnEditor::default(),
            gold_label: OnEditor::default(),
            game_world: OnEditor::default(),
            cached_wood: 0,
            cached_stone: 0,
            cached_food: 0,
            cached_gold: 0,
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.game_world.clone();
        if !game_world.is_instance_valid() {
            godot_warn!("ResourceHeader: game_world reference not set");
            return;
        }

        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let game_world = self.game_world.clone();
        if !game_world.is_instance_valid() {
            return;
        }

        let gw = game_world.bind();
        let wood = gw.wood();
        let stone = gw.stone();
        let food = gw.food();
        let gold = gw.gold();

        if wood != self.cached_wood {
            self.wood_label.set_text(format!("Wood: {}", wood).as_str());
            self.cached_wood = wood;
        }
        if stone != self.cached_stone {
            self.stone_label.set_text(format!("Stone: {}", stone).as_str());
            self.cached_stone = stone;
        }
        if food != self.cached_food {
            self.food_label.set_text(format!("Food: {}", food).as_str());
            self.cached_food = food;
        }
        if gold != self.cached_gold {
            self.gold_label.set_text(format!("Gold: {}", gold).as_str());
            self.cached_gold = gold;
        }
    }
}
