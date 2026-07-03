use godot::classes::{Control, IControl, Label};
use godot::obj::OnEditor;
use godot::prelude::*;
use crate::resources::{ResourceManager, ResourcesState};

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

    manager: Option<Gd<ResourceManager>>,
    cached: ResourcesState,

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
            manager: None,
            cached: ResourcesState::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let manager = self.base().get_node_as::<ResourceManager>("/root/ResourceManager");
        if !manager.is_instance_valid() {
            godot_warn!("ResourceHeader: /root/ResourceManager autoload not found");
            return;
        }

        self.refresh(&manager);
        self.cached.wood = manager.bind().wood();
        self.cached.stone = manager.bind().stone();
        self.cached.food = manager.bind().food();
        self.cached.gold = manager.bind().gold();
        self.manager = Some(manager);
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let Some(manager) = &self.manager else { return };

        let wood = manager.bind().wood();
        let stone = manager.bind().stone();
        let food = manager.bind().food();
        let gold = manager.bind().gold();

        if wood != self.cached.wood {
            self.wood_label.set_text(format!("Wood: {}", wood).as_str());
            self.cached.wood = wood;
        }
        if stone != self.cached.stone {
            self.stone_label.set_text(format!("Stone: {}", stone).as_str());
            self.cached.stone = stone;
        }
        if food != self.cached.food {
            self.food_label.set_text(format!("Food: {}", food).as_str());
            self.cached.food = food;
        }
        if gold != self.cached.gold {
            self.gold_label.set_text(format!("Gold: {}", gold).as_str());
            self.cached.gold = gold;
        }
    }
}

impl ResourceHeader {
    fn refresh(&mut self, manager: &Gd<ResourceManager>) {
        self.wood_label.set_text(format!("Wood: {}", manager.bind().wood()).as_str());
        self.stone_label.set_text(format!("Stone: {}", manager.bind().stone()).as_str());
        self.food_label.set_text(format!("Food: {}", manager.bind().food()).as_str());
        self.gold_label.set_text(format!("Gold: {}", manager.bind().gold()).as_str());
    }
}
