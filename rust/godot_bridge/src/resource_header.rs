use crate::game_world::GameWorld;
use game_engine::resources::{ResourceKind, ResourceSnapshot};
use godot::classes::{HBoxContainer, IHBoxContainer, Label};
use godot::obj::OnEditor;
use godot::prelude::*;

type ResourceLabels = (Gd<Label>, Gd<Label>, Gd<Label>, Gd<Label>);

#[derive(GodotClass)]
#[class(base = HBoxContainer)]
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

    cached_resources: ResourceSnapshot,

    base: Base<HBoxContainer>,
}

#[godot_api]
impl IHBoxContainer for ResourceHeader {
    fn init(base: Base<HBoxContainer>) -> Self {
        Self {
            wood_label: OnEditor::default(),
            stone_label: OnEditor::default(),
            food_label: OnEditor::default(),
            gold_label: OnEditor::default(),
            game_world: OnEditor::default(),
            cached_resources: ResourceSnapshot::default(),
            base,
        }
    }

    fn ready(&mut self) {
        if self.game_world_node().is_none() {
            godot_warn!("ResourceHeader: game_world reference not set");
            return;
        }
        if self.label_nodes().is_none() {
            godot_warn!("ResourceHeader: one or more label references are not set");
            return;
        }

        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let Some(game_world) = self.game_world_node() else {
            return;
        };
        let Some((mut wood_label, mut stone_label, mut food_label, mut gold_label)) =
            self.label_nodes()
        else {
            return;
        };

        let snapshot = game_world.bind().resource_snapshot();

        if snapshot.wood != self.cached_resources.wood {
            wood_label.set_text(resource_text(ResourceKind::Wood, snapshot.wood).as_str());
        }
        if snapshot.stone != self.cached_resources.stone {
            stone_label.set_text(resource_text(ResourceKind::Stone, snapshot.stone).as_str());
        }
        if snapshot.food != self.cached_resources.food {
            food_label.set_text(resource_text(ResourceKind::Food, snapshot.food).as_str());
        }
        if snapshot.gold != self.cached_resources.gold {
            gold_label.set_text(resource_text(ResourceKind::Gold, snapshot.gold).as_str());
        }

        self.cached_resources = snapshot;
    }
}

impl ResourceHeader {
    fn game_world_node(&self) -> Option<Gd<GameWorld>> {
        let game_world = self.game_world.clone();
        game_world.is_instance_valid().then_some(game_world)
    }

    fn label_nodes(&self) -> Option<ResourceLabels> {
        let wood_label = self.wood_label.clone();
        let stone_label = self.stone_label.clone();
        let food_label = self.food_label.clone();
        let gold_label = self.gold_label.clone();

        (wood_label.is_instance_valid()
            && stone_label.is_instance_valid()
            && food_label.is_instance_valid()
            && gold_label.is_instance_valid())
        .then_some((wood_label, stone_label, food_label, gold_label))
    }
}

fn resource_text(kind: ResourceKind, amount: u32) -> String {
    format!("{}: {}", kind.label(), amount)
}
