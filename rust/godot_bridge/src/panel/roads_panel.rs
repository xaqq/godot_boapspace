use crate::world::game_world::GameWorld;
use game_engine::resources::ResourceKind;
use game_engine::roads::RoadTier;
use godot::classes::{Button, IPanelContainer, Label, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct RoadsPanel {
    #[export]
    dirt_button: OnEditor<Gd<Button>>,
    #[export]
    cobblestone_button: OnEditor<Gd<Button>>,
    #[export]
    flagstone_button: OnEditor<Gd<Button>>,
    #[export]
    active_label: OnEditor<Gd<Label>>,
    #[export]
    stroke_label: OnEditor<Gd<Label>>,
    #[export]
    errors_label: OnEditor<Gd<Label>>,
    #[export]
    game_world: OnEditor<Gd<GameWorld>>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for RoadsPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            dirt_button: OnEditor::default(),
            cobblestone_button: OnEditor::default(),
            flagstone_button: OnEditor::default(),
            active_label: OnEditor::default(),
            stroke_label: OnEditor::default(),
            errors_label: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        connect_tier(
            self.dirt_button.clone(),
            self.game_world.clone(),
            RoadTier::DirtPath,
        );
        connect_tier(
            self.cobblestone_button.clone(),
            self.game_world.clone(),
            RoadTier::Cobblestone,
        );
        connect_tier(
            self.flagstone_button.clone(),
            self.game_world.clone(),
            RoadTier::Flagstone,
        );
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let status = self.game_world.bind().road_placement_status();
        self.active_label.clone().set_text(
            status
                .active_tier
                .map_or("Active road: None".to_owned(), |tier| {
                    format!("Active road: {}", tier.label())
                })
                .as_str(),
        );
        let cost = format_cost(status.aggregate_cost);
        self.stroke_label
            .clone()
            .set_text(format!("Stroke: {} cell(s) | Cost: {cost}", status.cell_count).as_str());
        let errors = status
            .errors
            .into_iter()
            .map(|(coord, error)| format!("({}, {}): {}", coord.x(), coord.y(), error.label()))
            .collect::<Vec<_>>()
            .join("\n");
        self.errors_label.clone().set_text(if errors.is_empty() {
            "Valid stroke"
        } else {
            errors.as_str()
        });
    }
}

fn connect_tier(button: Gd<Button>, game_world: Gd<GameWorld>, tier: RoadTier) {
    button
        .signals()
        .pressed()
        .connect_other(&game_world, move |game_world: &mut GameWorld| {
            game_world.start_road_placement(tier);
        });
}

fn format_cost(cost: game_engine::resources::ResourceAmounts) -> String {
    let parts = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let amount = cost.get(kind);
            (amount > 0).then(|| format!("{amount} {}", kind.label()))
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "None".to_owned()
    } else {
        parts.join(", ")
    }
}
