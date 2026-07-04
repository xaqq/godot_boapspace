use crate::game_world::GameWorld;
use godot::classes::{Control, IControl, Label};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct TileInfoPanel {
    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    type_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<Control>,
}

#[godot_api]
impl IControl for TileInfoPanel {
    fn init(base: Base<Control>) -> Self {
        Self {
            pos_label: OnEditor::default(),
            type_label: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.game_world.clone();
        let mut pos1 = self.pos_label.clone();
        let mut type1 = self.type_label.clone();

        game_world.signals().tile_selected().connect(
            move |x: i32, y: i32, type_name: GString, resource_name: GString| {
                pos1.set_text(format!("Cell: ({}, {})", x, y).as_str());
                if resource_name.is_empty() {
                    type1.set_text(format!("Type: {}", type_name).as_str());
                } else {
                    type1.set_text(
                        format!("Type: {}\nResource: {}", type_name, resource_name).as_str(),
                    );
                }
            },
        );

        let mut pos2 = self.pos_label.clone();
        let mut type2 = self.type_label.clone();
        game_world.signals().tile_deselected().connect(move || {
            pos2.set_text("Cell: None");
            type2.set_text("Type: --");
        });
    }
}
