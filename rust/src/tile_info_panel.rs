use godot::classes::{Control, IControl, Label};
use godot::obj::OnEditor;
use godot::prelude::*;
use crate::selected_tile::SelectedTile;

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct TileInfoPanel {
    #[export]
    selected_tile: OnEditor<Gd<SelectedTile>>,

    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    type_label: OnEditor<Gd<Label>>,

    base: Base<Control>,
}

#[godot_api]
impl IControl for TileInfoPanel {
    fn init(base: Base<Control>) -> Self {
        Self {
            selected_tile: OnEditor::default(),
            pos_label: OnEditor::default(),
            type_label: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let tile = self.selected_tile.clone();
        let tile = tile.bind();

        let pos_text = match (tile.cell_x(), tile.cell_y()) {
            (Some(x), Some(y)) => format!("Cell: ({}, {})", x, y),
            _ => "Cell: None".to_string(),
        };

        let type_text = match tile.type_name() {
            Some(name) => format!("Type: {}", name),
            None => "Type: --".to_string(),
        };

        self.pos_label.set_text(pos_text.as_str());
        self.type_label.set_text(type_text.as_str());
    }
}
