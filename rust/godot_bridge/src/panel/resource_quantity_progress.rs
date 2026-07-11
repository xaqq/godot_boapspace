use super::resource_quantity::ResourceQuantity;
use game_engine::resources::ResourceKind;
use godot::classes::{HBoxContainer, IHBoxContainer, ProgressBar};
use godot::obj::OnEditor;
use godot::prelude::*;

const PROGRESS_BAR_MAX: f64 = 100.0;

#[derive(GodotClass)]
#[class(base = HBoxContainer)]
pub(crate) struct ResourceQuantityProgress {
    #[export]
    kind: ResourceKind,

    #[export]
    deposited_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    required_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    progress_bar: OnEditor<Gd<ProgressBar>>,

    base: Base<HBoxContainer>,
}

#[godot_api]
impl IHBoxContainer for ResourceQuantityProgress {
    fn init(base: Base<HBoxContainer>) -> Self {
        Self {
            kind: ResourceKind::Wood,
            deposited_quantity: OnEditor::default(),
            required_quantity: OnEditor::default(),
            progress_bar: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let mut progress_bar = self.progress_bar.clone();
        progress_bar.set_min(0.0);
        progress_bar.set_max(PROGRESS_BAR_MAX);
        progress_bar.set_show_percentage(false);

        self.refresh_resource_kind();
        self.set_amounts(0, 0);
    }
}

impl ResourceQuantityProgress {
    pub(crate) fn set_resource_kind(&mut self, kind: ResourceKind) {
        self.kind = kind;
        self.refresh_resource_kind();
    }

    pub(crate) fn set_amounts(&mut self, deposited: u32, required: u32) {
        let mut deposited_quantity = self.deposited_quantity.clone();
        deposited_quantity.bind_mut().set_amount(deposited);

        let mut required_quantity = self.required_quantity.clone();
        required_quantity.bind_mut().set_amount(required);

        let mut progress_bar = self.progress_bar.clone();
        progress_bar.set_value(progress_bar_value(deposited, required));

        let tooltip = format!("{}: {}/{}", self.kind.label(), deposited, required);
        self.base_mut().set_tooltip_text(tooltip.as_str());
        progress_bar.set_tooltip_text(tooltip.as_str());
    }

    fn refresh_resource_kind(&mut self) {
        let mut deposited_quantity = self.deposited_quantity.clone();
        deposited_quantity.bind_mut().set_resource_kind(self.kind);

        let mut required_quantity = self.required_quantity.clone();
        required_quantity.bind_mut().set_resource_kind(self.kind);
    }
}

fn progress_bar_value(deposited: u32, required: u32) -> f64 {
    if required == 0 {
        return 0.0;
    }

    f64::from(deposited.min(required)) / f64::from(required) * PROGRESS_BAR_MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_value_is_zero_without_requirement() {
        assert_eq!(progress_bar_value(10, 0), 0.0);
    }

    #[test]
    fn progress_bar_value_scales_deposited_over_required() {
        assert_eq!(progress_bar_value(5, 20), 25.0);
        assert_eq!(progress_bar_value(20, 20), 100.0);
    }

    #[test]
    fn progress_bar_value_clamps_over_deposited_resources() {
        assert_eq!(progress_bar_value(40, 20), 100.0);
    }
}
