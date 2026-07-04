use godot::classes::{Control, IControl, Label};
use godot::obj::OnEditor;
use godot::prelude::*;
use crate::game_world::GameWorld;

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct TileInfoPanel {
    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    type_label: OnEditor<Gd<Label>>,

    game_world: Option<Gd<GameWorld>>,

    base: Base<Control>,
}

#[godot_api]
impl IControl for TileInfoPanel {
    fn init(base: Base<Control>) -> Self {
        Self {
            pos_label: OnEditor::default(),
            type_label: OnEditor::default(),
            game_world: None,
            base,
        }
    }

    fn ready(&mut self) {
        if let Some(parent) = self.base().get_parent() {
            let world = parent.get_node_as::<GameWorld>("GameWorld");
            if world.is_instance_valid() {
                self.game_world = Some(world);
            }
        }
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let Some(world) = &self.game_world else { return };
        let world = world.bind();

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

        self.pos_label.set_text(pos_text.as_str());
        self.type_label.set_text(type_text.as_str());
    }
}
