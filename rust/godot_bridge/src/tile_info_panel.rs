use crate::game_world::GameWorld;
use godot::classes::{IPanelContainer, Label, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct TileInfoPanel {
    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    resource_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for TileInfoPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            pos_label: OnEditor::default(),
            resource_label: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.game_world.clone();
        let pos_label = self.pos_label.clone();
        let resource_label = self.resource_label.clone();

        let mut selected_pos_label = pos_label.clone();
        let mut selected_resource_label = resource_label.clone();
        game_world
            .signals()
            .tile_selected()
            .connect(move |x, y, resource_name| {
                selected_pos_label.set_text(format!("Cell: ({x}, {y})").as_str());
                selected_resource_label.set_text(resource_text(&resource_name).as_str());
            });

        let mut deselected_pos_label = pos_label;
        let mut deselected_resource_label = resource_label;
        game_world.signals().tile_deselected().connect(move || {
            deselected_pos_label.set_text("Cell: None");
            deselected_resource_label.set_text("");
        });
    }
}

fn resource_text(resource_name: &GString) -> String {
    if resource_name.is_empty() {
        String::new()
    } else {
        format!("Resource: {resource_name}")
    }
}
