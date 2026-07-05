use godot::classes::{Button, Control, IControl, SceneTree};
use godot::global::Error;
use godot::obj::OnEditor;
use godot::prelude::*;

const GAME_WORLD_SCENE: &str = "res://game_world.tscn";

#[derive(GodotClass)]
#[class(base = Control)]
struct RootMenu {
    #[export]
    new_game_button: OnEditor<Gd<Button>>,

    #[export]
    exit_button: OnEditor<Gd<Button>>,

    #[export]
    settings_button: OnEditor<Gd<Button>>,

    base: Base<Control>,
}

#[godot_api]
impl IControl for RootMenu {
    fn init(base: Base<Control>) -> Self {
        Self {
            new_game_button: OnEditor::default(),
            exit_button: OnEditor::default(),
            settings_button: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let new_game_btn = self.new_game_button.clone();
        let mut new_game_tree = self.base().get_tree();
        new_game_btn.signals().pressed().connect(move || {
            change_scene(&mut new_game_tree, GAME_WORLD_SCENE);
        });

        let exit_btn = self.exit_button.clone();
        let mut exit_tree = self.base().get_tree();
        exit_btn.signals().pressed().connect(move || {
            exit_tree.quit();
        });

        let settings_btn = self.settings_button.clone();
        settings_btn.signals().pressed().connect(|| {
            godot_print!("Settings pressed");
        });
    }
}

fn change_scene(tree: &mut Gd<SceneTree>, path: &str) {
    let error = tree.change_scene_to_file(path);
    if error != Error::OK {
        godot_error!("RootMenu: failed to change scene to {path}: {error:?}");
    }
}
