use crate::world::game_world::GameWorld;
use game_engine::resources::{GameResources, ResourceKind};
use godot::classes::{HBoxContainer, IHBoxContainer, Label};
use godot::obj::OnEditor;
use godot::prelude::*;

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

    cached_amounts: [Option<u32>; ResourceKind::ALL.len()],

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
            cached_amounts: [None; ResourceKind::ALL.len()],
            base,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let game_world = self.game_world.clone();
        let mut wood_label = self.wood_label.clone();
        let mut stone_label = self.stone_label.clone();
        let mut food_label = self.food_label.clone();
        let mut gold_label = self.gold_label.clone();

        let Some(amounts) = game_world.bind().with_rendered_surface_world(|world| {
            let resources = world.resource::<GameResources>();
            ResourceKind::ALL.map(|kind| resources.get(kind))
        }) else {
            return;
        };

        self.update_label(
            &mut wood_label,
            ResourceKind::Wood,
            amounts[resource_index(ResourceKind::Wood)],
        );
        self.update_label(
            &mut stone_label,
            ResourceKind::Stone,
            amounts[resource_index(ResourceKind::Stone)],
        );
        self.update_label(
            &mut food_label,
            ResourceKind::Food,
            amounts[resource_index(ResourceKind::Food)],
        );
        self.update_label(
            &mut gold_label,
            ResourceKind::Gold,
            amounts[resource_index(ResourceKind::Gold)],
        );
    }
}

impl ResourceHeader {
    fn update_label(&mut self, label: &mut Gd<Label>, kind: ResourceKind, amount: u32) {
        let cached_amount = &mut self.cached_amounts[resource_index(kind)];
        if *cached_amount != Some(amount) {
            label.set_text(resource_text(kind, amount).as_str());
            *cached_amount = Some(amount);
        }
    }
}

fn resource_text(kind: ResourceKind, amount: u32) -> String {
    format!("{}: {}", kind.label(), amount)
}

fn resource_index(kind: ResourceKind) -> usize {
    kind as usize
}
