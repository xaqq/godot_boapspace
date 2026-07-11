use super::resource_tooltip::ResourceTooltip;
use crate::assets::{load_packed_scene, load_texture, resource_asset_path};
use game_engine::resources::ResourceKind;
use godot::classes::{control, HBoxContainer, IHBoxContainer, Label, Object, TextureRect};
use godot::obj::OnEditor;
use godot::prelude::*;

const RESOURCE_TOOLTIP_SCENE_PATH: &str = "res://panel/resource_tooltip.tscn";

#[derive(GodotClass)]
#[class(base = HBoxContainer)]
pub(crate) struct ResourceQuantity {
    #[export]
    kind: ResourceKind,

    #[export]
    icon_rect: OnEditor<Gd<TextureRect>>,

    #[export]
    amount_label: OnEditor<Gd<Label>>,

    base: Base<HBoxContainer>,
}

#[godot_api]
impl IHBoxContainer for ResourceQuantity {
    fn init(base: Base<HBoxContainer>) -> Self {
        Self {
            kind: ResourceKind::Wood,
            icon_rect: OnEditor::default(),
            amount_label: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        self.refresh_resource();
        self.set_amount(0);
    }

    fn make_custom_tooltip(&self, _for_text: GString) -> Option<Gd<Object>> {
        let scene = load_packed_scene(RESOURCE_TOOLTIP_SCENE_PATH, "ResourceQuantity")?;
        let Some(node) = scene.instantiate() else {
            godot_error!(
                "ResourceQuantity: failed to instantiate tooltip scene {RESOURCE_TOOLTIP_SCENE_PATH}"
            );
            return None;
        };

        match node.try_cast::<ResourceTooltip>() {
            Ok(mut tooltip) => {
                tooltip.bind_mut().set_resource(self.kind);
                Some(tooltip.upcast::<Object>())
            }
            Err(node) => {
                godot_error!(
                    "ResourceQuantity: instantiated tooltip scene root as {}, expected ResourceTooltip",
                    node.get_class()
                );
                None
            }
        }
    }
}

impl ResourceQuantity {
    pub(crate) fn set_resource_kind(&mut self, kind: ResourceKind) {
        self.kind = kind;
        self.refresh_resource();
    }

    pub(crate) fn set_amount(&mut self, amount: u32) {
        self.set_display_text(amount_text(amount).as_str());
    }

    pub(crate) fn set_display_text(&mut self, text: &str) {
        let mut amount_label = self.amount_label.clone();
        amount_label.set_text(text);
        amount_label.show();
    }

    pub(crate) fn hide_amount(&mut self) {
        self.amount_label.clone().hide();
    }

    pub(crate) fn set_mouse_passthrough(&mut self) {
        self.base_mut()
            .set_mouse_filter(control::MouseFilter::IGNORE);
        self.icon_rect
            .clone()
            .set_mouse_filter(control::MouseFilter::IGNORE);
        self.amount_label
            .clone()
            .set_mouse_filter(control::MouseFilter::IGNORE);
    }

    pub(crate) fn show_quantity(&mut self) {
        self.base_mut().show();
    }

    pub(crate) fn hide_quantity(&mut self) {
        self.base_mut().hide();
    }

    fn refresh_resource(&mut self) {
        let mut icon_rect = self.icon_rect.clone();
        let tooltip_text = self.kind.label();

        if let Some(texture) = load_texture(resource_asset_path(self.kind), "ResourceQuantity") {
            icon_rect.set_texture(&texture);
        }

        self.base_mut().set_tooltip_text(tooltip_text);
    }
}

fn amount_text(amount: u32) -> String {
    amount.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_text_is_amount_only() {
        assert_eq!(amount_text(0), "0");
        assert_eq!(amount_text(100), "100");
        assert_eq!(amount_text(u32::MAX), "4294967295");
    }
}
