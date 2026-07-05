use game_engine::resources::ResourceKind;
use godot::classes::{IMarginContainer, MarginContainer, RichTextLabel};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = MarginContainer)]
pub(crate) struct ResourceTooltip {
    #[export]
    text_label: OnEditor<Gd<RichTextLabel>>,

    base: Base<MarginContainer>,
}

#[godot_api]
impl IMarginContainer for ResourceTooltip {
    fn init(base: Base<MarginContainer>) -> Self {
        Self {
            text_label: OnEditor::default(),
            base,
        }
    }
}

impl ResourceTooltip {
    pub(crate) fn set_resource(&mut self, kind: ResourceKind) {
        let mut text_label = self.text_label.clone();
        text_label.parse_bbcode(resource_tooltip_text(kind).as_str());
    }
}

fn resource_tooltip_text(kind: ResourceKind) -> String {
    format!("[b]{}[/b]\n{}", kind.label(), kind.description())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_tooltip_text_contains_bold_label_and_description() {
        assert_eq!(
            resource_tooltip_text(ResourceKind::Wood),
            "[b]Wood[/b]\nFlexible timber used for basic construction, repairs, and early infrastructure."
        );
    }
}
