use godot::classes::{Control, IControl, Label};
use godot::obj::OnEditor;
use godot::prelude::*;
use godot::signal::ConnectHandle;
use crate::game_world::GameWorld;

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct TileInfoPanel {
    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    type_label: OnEditor<Gd<Label>>,

    _select_handle: Option<ConnectHandle>,
    _deselect_handle: Option<ConnectHandle>,

    base: Base<Control>,
}

#[godot_api]
impl IControl for TileInfoPanel {
    fn init(base: Base<Control>) -> Self {
        Self {
            pos_label: OnEditor::default(),
            type_label: OnEditor::default(),
            _select_handle: None,
            _deselect_handle: None,
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.base().get_node_as::<GameWorld>(
            "../SubViewportContainer/SubViewport/GameWorld",
        );
        if !game_world.is_instance_valid() {
            return;
        }

        let mut pos1 = self.pos_label.clone();
        let mut type1 = self.type_label.clone();
        let h1 = game_world.signals().tile_selected().connect(
            move |x: i32, y: i32, type_name: GString| {
                pos1.set_text(format!("Cell: ({}, {})", x, y).as_str());
                type1.set_text(format!("Type: {}", type_name).as_str());
            },
        );
        self._select_handle = Some(h1);

        let mut pos2 = self.pos_label.clone();
        let mut type2 = self.type_label.clone();
        let h2 = game_world.signals().tile_deselected().connect(
            move || {
                pos2.set_text("Cell: None");
                type2.set_text("Type: --");
            },
        );
        self._deselect_handle = Some(h2);
    }
}
