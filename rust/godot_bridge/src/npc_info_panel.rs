use crate::game_world::GameWorld;
use godot::classes::{IPanelContainer, Label, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct NpcInfoPanel {
    #[export]
    name_label: OnEditor<Gd<Label>>,

    #[export]
    age_label: OnEditor<Gd<Label>>,

    #[export]
    birth_day_label: OnEditor<Gd<Label>>,

    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for NpcInfoPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            name_label: OnEditor::default(),
            age_label: OnEditor::default(),
            birth_day_label: OnEditor::default(),
            pos_label: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.game_world.clone();
        let name_label = self.name_label.clone();
        let age_label = self.age_label.clone();
        let birth_day_label = self.birth_day_label.clone();
        let pos_label = self.pos_label.clone();

        let selected_game_world = game_world.clone();
        let mut selected_name_label = name_label.clone();
        let mut selected_age_label = age_label.clone();
        let mut selected_birth_day_label = birth_day_label.clone();
        let mut selected_pos_label = pos_label.clone();
        game_world
            .signals()
            .npc_selected()
            .connect(move |npc_entity_id| {
                let game_world = selected_game_world.bind();
                let name_text = game_world.npc_name_text(npc_entity_id).to_string();
                let age_text = game_world.npc_age_text(npc_entity_id).to_string();
                let birth_day_text = game_world.npc_birth_day_text(npc_entity_id).to_string();
                let position_text = game_world.npc_position_text(npc_entity_id).to_string();
                selected_name_label.set_text(name_text.as_str());
                selected_age_label.set_text(age_text.as_str());
                selected_birth_day_label.set_text(birth_day_text.as_str());
                selected_pos_label.set_text(position_text.as_str());
            });

        let mut deselected_name_label = name_label;
        let mut deselected_age_label = age_label;
        let mut deselected_birth_day_label = birth_day_label;
        let mut deselected_pos_label = pos_label;
        game_world.signals().npc_deselected().connect(move || {
            deselected_name_label.set_text("Name: None");
            deselected_age_label.set_text("");
            deselected_birth_day_label.set_text("");
            deselected_pos_label.set_text("Cell: None");
        });
    }
}
