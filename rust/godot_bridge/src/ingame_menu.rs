use godot::classes::{Button, Control, IControl, InputEvent, SceneTree};
use godot::global::Error;
use godot::obj::OnEditor;
use godot::prelude::*;

const MAIN_MENU_SCENE: &str = "res://main_ui.tscn";
const ACTION_MENU_TOGGLE: &str = "menu_toggle";

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct IngameMenu {
    #[export]
    continue_button: OnEditor<Gd<Button>>,

    #[export]
    return_button: OnEditor<Gd<Button>>,

    #[export]
    exit_button: OnEditor<Gd<Button>>,

    base: Base<Control>,
}

#[godot_api]
impl IControl for IngameMenu {
    fn init(base: Base<Control>) -> Self {
        Self {
            continue_button: OnEditor::default(),
            return_button: OnEditor::default(),
            exit_button: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let continue_btn = self.continue_button.clone();
        continue_btn
            .signals()
            .pressed()
            .connect_other(self, Self::hide_menu);

        let return_btn = self.return_button.clone();
        let mut return_tree = self.base().get_tree();
        return_btn.signals().pressed().connect(move || {
            change_scene(&mut return_tree, MAIN_MENU_SCENE);
        });

        let exit_btn = self.exit_button.clone();
        let mut exit_tree = self.base().get_tree();
        exit_btn.signals().pressed().connect(move || {
            exit_tree.quit();
        });
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if event.is_action_pressed(ACTION_MENU_TOGGLE) {
            if self.base().is_visible() {
                self.base_mut().hide();
            } else {
                self.base_mut().show();
            }
        }
    }
}

impl IngameMenu {
    fn hide_menu(&mut self) {
        self.base_mut().hide();
    }
}

fn change_scene(tree: &mut Gd<SceneTree>, path: &str) {
    let error = tree.change_scene_to_file(path);
    if error != Error::OK {
        godot_error!("IngameMenu: failed to change scene to {path}: {error:?}");
    }
}
