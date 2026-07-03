use godot::classes::{Control, IControl, Label};
use godot::obj::OnEditor;
use godot::prelude::*;
use crate::game_world::GameWorld;

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct TileInfoPanel {
    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    type_label: OnEditor<Gd<Label>>,

    cached_pos: String,
    cached_type: String,

    base: Base<Control>,
}

#[godot_api]
impl IControl for TileInfoPanel {
    fn init(base: Base<Control>) -> Self {
        Self {
            game_world: OnEditor::default(),
            pos_label: OnEditor::default(),
            type_label: OnEditor::default(),
            cached_pos: String::new(),
            cached_type: String::new(),
            base,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let world = self.game_world.bind();

        let pos_text = if world.has_selection() {
            format!("Cell: ({}, {})", world.selected_cell_x(), world.selected_cell_y())
        } else {
            "Cell: None".to_string()
        };

        let type_text = if world.has_selection() {
            format!("Type: {}", world.selected_cell_type_name())
        } else {
            "Type: --".to_string()
        };

        if pos_text != self.cached_pos {
            self.pos_label.set_text(pos_text.as_str());
            self.cached_pos = pos_text;
        }
        if type_text != self.cached_type {
            self.type_label.set_text(type_text.as_str());
            self.cached_type = type_text;
        }
    }
}
