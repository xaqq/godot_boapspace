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
        let Some(game_world) = self.game_world_node() else {
            godot_warn!("TileInfoPanel: game_world reference not set");
            return;
        };
        let Some((mut pos1, mut type1)) = self.label_nodes() else {
            godot_warn!("TileInfoPanel: one or more label references are not set");
            return;
        };

        game_world
            .signals()
            .tile_selected()
            .connect(move |x: i32, y: i32, type_name: GString| {
                if pos1.is_instance_valid() {
                    pos1.set_text(format!("Cell: ({}, {})", x, y).as_str());
                }
                if type1.is_instance_valid() {
                    type1.set_text(format!("Type: {}", type_name).as_str());
                }
            });

        let Some((mut pos2, mut type2)) = self.label_nodes() else {
            return;
        };
        game_world.signals().tile_deselected().connect(move || {
            if pos2.is_instance_valid() {
                pos2.set_text("Cell: None");
            }
            if type2.is_instance_valid() {
                type2.set_text("Type: --");
            }
        });
    }
}

impl TileInfoPanel {
    fn game_world_node(&self) -> Option<Gd<GameWorld>> {
        let game_world = self.game_world.clone();
        game_world.is_instance_valid().then_some(game_world)
    }

    fn label_nodes(&self) -> Option<(Gd<Label>, Gd<Label>)> {
        let pos_label = self.pos_label.clone();
        let type_label = self.type_label.clone();

        (pos_label.is_instance_valid() && type_label.is_instance_valid())
            .then_some((pos_label, type_label))
    }
}
