use crate::assets::{load_texture, resource_asset_path};
use game_engine::resources::ResourceKind;
use godot::classes::{HBoxContainer, IHBoxContainer, Label, TextureRect};
use godot::obj::OnEditor;
use godot::prelude::*;

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
        let mut icon_rect = self.icon_rect.clone();
        let mut amount_label = self.amount_label.clone();

        if let Some(texture) = load_texture(resource_asset_path(self.kind), "ResourceQuantity") {
            icon_rect.set_texture(&texture);
        }

        amount_label.set_text(amount_text(0).as_str());
    }
}

impl ResourceQuantity {
    pub(crate) fn set_amount(&mut self, amount: u32) {
        let mut amount_label = self.amount_label.clone();
        amount_label.set_text(amount_text(amount).as_str());
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
