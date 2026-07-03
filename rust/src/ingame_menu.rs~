use godot::classes::{Button, Control, IControl, InputEvent, InputEventKey};
use godot::global::Key;
use godot::obj::OnEditor;
use godot::prelude::*;

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
        let mut ctrl = self.base().clone();
        let continue_btn: &mut Gd<Button> = &mut *self.continue_button;
        continue_btn.signals().pressed().connect(move || {
            ctrl.hide();
        });

        let mut return_tree = self.base().get_tree();
        let return_btn: &mut Gd<Button> = &mut *self.return_button;
        return_btn.signals().pressed().connect(move || {
            return_tree.change_scene_to_file("res://main_ui.tscn");
        });

        let mut exit_tree = self.base().get_tree();
        let exit_btn: &mut Gd<Button> = &mut *self.exit_button;
        exit_btn.signals().pressed().connect(move || {
            exit_tree.quit();
        });
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        let Ok(key_event) = event.try_cast::<InputEventKey>() else { return };
        if key_event.get_keycode() == Key::ESCAPE
            && key_event.is_pressed()
            && !key_event.is_echo()
        {
            if self.base().is_visible() {
                self.base_mut().hide();
            } else {
                self.base_mut().show();
            }
        }
    }
}
