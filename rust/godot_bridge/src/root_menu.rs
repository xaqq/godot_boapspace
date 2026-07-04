use godot::classes::{Button, Control, IControl};
use godot::obj::OnEditor;
use godot::prelude::*;

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
        let mut new_game_tree = self.base().get_tree();
        let new_game_btn: &mut Gd<Button> = &mut *self.new_game_button;
        new_game_btn.signals().pressed().connect(move || {
            new_game_tree.change_scene_to_file("res://game_world.tscn");
        });

        let mut exit_tree = self.base().get_tree();
        let exit_btn: &mut Gd<Button> = &mut *self.exit_button;
        exit_btn.signals().pressed().connect(move || {
            exit_tree.quit();
        });

        let settings_btn: &mut Gd<Button> = &mut *self.settings_button;
        settings_btn.signals().pressed().connect(|| {
            godot_print!("Settings pressed");
        });
    }
}
