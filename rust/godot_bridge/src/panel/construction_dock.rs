use crate::assets::{building_asset_path, load_texture, road_asset_path};
use crate::world::game_world::{
    BuildingPlacementFeedback, ConstructionPlacementStatus, ConstructionTool, GameWorld,
};
use game_engine::buildings::{BuildingKind, BuildingPlacementError};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::roads::RoadTier;
use godot::classes::{
    control, AtlasTexture, Button, Control, HBoxContainer, IControl, InputEvent, Label,
    PanelContainer, Texture2D,
};
use godot::obj::{NewAlloc, OnEditor};
use godot::prelude::*;

const ACTION_CONSTRUCTION_TOGGLE: &str = "construction_toggle";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstructionCategory {
    Storage,
    Production,
    Processing,
    Housing,
    Civic,
    Roads,
}

impl ConstructionCategory {
    const ALL: [Self; 6] = [
        Self::Storage,
        Self::Production,
        Self::Processing,
        Self::Housing,
        Self::Civic,
        Self::Roads,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Storage => "Storage",
            Self::Production => "Production",
            Self::Processing => "Processing",
            Self::Housing => "Housing",
            Self::Civic => "Civic",
            Self::Roads => "Roads",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConstructionCard {
    tool: ConstructionTool,
    description: &'static str,
}

const STORAGE_CARDS: [ConstructionCard; 2] = [
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::Depot),
        description: "Stores up to 500 units of colony resources.",
    },
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::Warehouse),
        description: "Stores up to 2,000 units of colony resources.",
    },
];
const PRODUCTION_CARDS: [ConstructionCard; 2] = [
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::Farm),
        description: "Supports connected Fields that grow and harvest Crops.",
    },
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::ForesterLodge),
        description: "Supports connected Tree Plots that grow and harvest Wood.",
    },
];
const PROCESSING_CARDS: [ConstructionCard; 3] = [
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::Sawmill),
        description: "Processes Wood into Planks.",
    },
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::Stoneworks),
        description: "Processes Stone into Stone Blocks.",
    },
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::Kitchen),
        description: "Processes Crops or Wild Berries into Food.",
    },
];
const HOUSING_CARDS: [ConstructionCard; 3] = [
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::SmallHouse),
        description: "Provides housing for 2 colonists.",
    },
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::MediumHouse),
        description: "Provides housing for 4 colonists.",
    },
    ConstructionCard {
        tool: ConstructionTool::Building(BuildingKind::LargeHouse),
        description: "Provides housing for 8 colonists.",
    },
];
const CIVIC_CARDS: [ConstructionCard; 1] = [ConstructionCard {
    tool: ConstructionTool::Building(BuildingKind::TownHall),
    description: "A central civic landmark for the colony.",
}];
const ROAD_CARDS: [ConstructionCard; 3] = [
    ConstructionCard {
        tool: ConstructionTool::Road(RoadTier::DirtPath),
        description: "Increases movement speed to 1.5× normal.",
    },
    ConstructionCard {
        tool: ConstructionTool::Road(RoadTier::Cobblestone),
        description: "Increases movement speed to 2× normal.",
    },
    ConstructionCard {
        tool: ConstructionTool::Road(RoadTier::Flagstone),
        description: "Increases movement speed to 3× normal.",
    },
];

fn cards_for(category: ConstructionCategory) -> &'static [ConstructionCard] {
    match category {
        ConstructionCategory::Storage => &STORAGE_CARDS,
        ConstructionCategory::Production => &PRODUCTION_CARDS,
        ConstructionCategory::Processing => &PROCESSING_CARDS,
        ConstructionCategory::Housing => &HOUSING_CARDS,
        ConstructionCategory::Civic => &CIVIC_CARDS,
        ConstructionCategory::Roads => &ROAD_CARDS,
    }
}

struct CardControl {
    tool: ConstructionTool,
    button: Gd<Button>,
}

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct ConstructionDock {
    #[export]
    drawer: OnEditor<Gd<PanelContainer>>,

    #[export]
    cards_container: OnEditor<Gd<HBoxContainer>>,

    #[export]
    build_button: OnEditor<Gd<Button>>,

    #[export]
    cancel_button: OnEditor<Gd<Button>>,

    #[export]
    cancel_hint: OnEditor<Gd<Label>>,

    #[export]
    active_label: OnEditor<Gd<Label>>,

    #[export]
    status_label: OnEditor<Gd<Label>>,

    #[export]
    storage_button: OnEditor<Gd<Button>>,

    #[export]
    production_button: OnEditor<Gd<Button>>,

    #[export]
    processing_button: OnEditor<Gd<Button>>,

    #[export]
    housing_button: OnEditor<Gd<Button>>,

    #[export]
    civic_button: OnEditor<Gd<Button>>,

    #[export]
    roads_button: OnEditor<Gd<Button>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    category: ConstructionCategory,
    card_controls: Vec<CardControl>,
    last_status: Option<ConstructionPlacementStatus>,
    base: Base<Control>,
}

#[godot_api]
impl IControl for ConstructionDock {
    fn init(base: Base<Control>) -> Self {
        Self {
            drawer: OnEditor::default(),
            cards_container: OnEditor::default(),
            build_button: OnEditor::default(),
            cancel_button: OnEditor::default(),
            cancel_hint: OnEditor::default(),
            active_label: OnEditor::default(),
            status_label: OnEditor::default(),
            storage_button: OnEditor::default(),
            production_button: OnEditor::default(),
            processing_button: OnEditor::default(),
            housing_button: OnEditor::default(),
            civic_button: OnEditor::default(),
            roads_button: OnEditor::default(),
            game_world: OnEditor::default(),
            category: ConstructionCategory::Storage,
            card_controls: Vec::new(),
            last_status: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.build_button
            .clone()
            .signals()
            .pressed()
            .connect_other(self, Self::toggle_drawer);
        self.cancel_button
            .clone()
            .signals()
            .pressed()
            .connect_other(self, Self::cancel_placement);

        self.connect_category_button(self.storage_button.clone(), ConstructionCategory::Storage);
        self.connect_category_button(
            self.production_button.clone(),
            ConstructionCategory::Production,
        );
        self.connect_category_button(
            self.processing_button.clone(),
            ConstructionCategory::Processing,
        );
        self.connect_category_button(self.housing_button.clone(), ConstructionCategory::Housing);
        self.connect_category_button(self.civic_button.clone(), ConstructionCategory::Civic);
        self.connect_category_button(self.roads_button.clone(), ConstructionCategory::Roads);

        for (category, mut button) in self.category_buttons() {
            button.set_text(category.label());
        }

        self.drawer.clone().hide();
        self.select_category(ConstructionCategory::Storage);
        self.refresh_status();
        self.base_mut().set_process(true);
        self.base_mut().set_process_unhandled_input(true);
    }

    fn process(&mut self, _delta: f64) {
        self.refresh_status();
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if !event.is_action_pressed(ACTION_CONSTRUCTION_TOGGLE) {
            return;
        }
        self.toggle_drawer();
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
    }
}

impl ConstructionDock {
    fn connect_category_button(&mut self, button: Gd<Button>, category: ConstructionCategory) {
        button
            .signals()
            .pressed()
            .connect_other(self, move |dock| dock.select_category(category));
    }

    fn toggle_drawer(&mut self) {
        if self.drawer.clone().is_visible() {
            self.drawer.clone().hide();
        } else {
            self.drawer.clone().show();
        }
    }

    fn select_category(&mut self, category: ConstructionCategory) {
        self.category = category;
        for (candidate, mut button) in self.category_buttons() {
            button.set_pressed_no_signal(candidate == category);
        }
        self.rebuild_cards();
    }

    fn category_buttons(&self) -> [(ConstructionCategory, Gd<Button>); 6] {
        let buttons = [
            self.storage_button.clone(),
            self.production_button.clone(),
            self.processing_button.clone(),
            self.housing_button.clone(),
            self.civic_button.clone(),
            self.roads_button.clone(),
        ];
        std::array::from_fn(|index| (ConstructionCategory::ALL[index], buttons[index].clone()))
    }

    fn rebuild_cards(&mut self) {
        for mut control in self.card_controls.drain(..) {
            control.button.queue_free();
        }

        let active_tool = self
            .game_world
            .bind()
            .construction_placement_status()
            .active_tool;
        let mut container = self.cards_container.clone();
        for card in cards_for(self.category) {
            let mut button = Button::new_alloc();
            button.set_custom_minimum_size(Vector2::new(210.0, 116.0));
            button.set_text(card_button_text(*card).as_str());
            button.set_tooltip_text(card_tooltip(*card).as_str());
            button.set_toggle_mode(true);
            button.set_pressed_no_signal(active_tool == Some(card.tool));
            button.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
            if let Some(texture) = card_texture(card.tool) {
                button.set_button_icon(&texture);
                button.set_expand_icon(true);
            }
            let tool = card.tool;
            button
                .signals()
                .pressed()
                .connect_other(self, move |dock| dock.select_tool(tool));
            container.add_child(&button);
            self.card_controls.push(CardControl { tool, button });
        }
    }

    fn select_tool(&mut self, tool: ConstructionTool) {
        let mut game_world = self.game_world.bind_mut();
        match tool {
            ConstructionTool::Building(kind) => game_world.start_building_placement(kind),
            ConstructionTool::Road(tier) => game_world.start_road_placement(tier),
            ConstructionTool::Field | ConstructionTool::TreePlot => return,
        }
        drop(game_world);
        self.last_status = None;
        self.refresh_status();
    }

    fn cancel_placement(&mut self) {
        self.game_world.bind_mut().cancel_construction_placement();
        self.last_status = None;
        self.refresh_status();
    }

    fn refresh_status(&mut self) {
        let status = self.game_world.bind().construction_placement_status();
        if self.last_status.as_ref() == Some(&status) {
            return;
        }

        for control in &mut self.card_controls {
            control
                .button
                .set_pressed_no_signal(status.active_tool == Some(control.tool));
        }

        let Some(tool) = status.active_tool else {
            self.active_label.clone().hide();
            self.status_label.clone().hide();
            self.cancel_hint.clone().hide();
            self.cancel_button.clone().hide();
            self.last_status = Some(status);
            return;
        };

        self.active_label.clone().show();
        self.status_label.clone().show();
        self.cancel_hint.clone().show();
        self.cancel_button.clone().show();
        self.active_label
            .clone()
            .set_text(format!("Placing: {}", tool.label()).as_str());

        let (status_text, tooltip) = placement_status_text(&status);
        self.status_label.clone().set_text(status_text.as_str());
        self.status_label.clone().set_tooltip_text(tooltip.as_str());
        self.last_status = Some(status);
    }
}

fn card_texture(tool: ConstructionTool) -> Option<Gd<Texture2D>> {
    match tool {
        ConstructionTool::Building(kind) => {
            load_texture(building_asset_path(kind), "ConstructionDock")
        }
        ConstructionTool::Road(tier) => {
            let texture = load_texture(road_asset_path(tier), "ConstructionDock")?;
            let tile_size = texture.get_width() as f32 / 4.0;
            let mut atlas = AtlasTexture::new_gd();
            atlas.set_atlas(&texture);
            // East + west connectivity (mask 10) is a representative straight road tile.
            atlas.set_region(Rect2::new(
                Vector2::new(tile_size * 2.0, tile_size * 2.0),
                Vector2::new(tile_size, tile_size),
            ));
            Some(atlas.upcast())
        }
        ConstructionTool::Field | ConstructionTool::TreePlot => None,
    }
}

fn card_button_text(card: ConstructionCard) -> String {
    match card.tool {
        ConstructionTool::Building(kind) => {
            let definition = kind.definition();
            format!(
                "{}\n{}×{} · {}",
                ConstructionTool::Building(kind).label(),
                definition.width(),
                definition.height(),
                format_cost(definition.construction_cost())
            )
        }
        ConstructionTool::Road(tier) => format!(
            "{}\nPer tile · {}",
            tier.label(),
            format_cost(tier.material_cost())
        ),
        ConstructionTool::Field | ConstructionTool::TreePlot => String::new(),
    }
}

fn card_tooltip(card: ConstructionCard) -> String {
    format!(
        "{}\n{}",
        card_button_text(card).replace('\n', " — "),
        card.description
    )
}

fn placement_status_text(status: &ConstructionPlacementStatus) -> (String, String) {
    match status.building_feedback {
        Some(BuildingPlacementFeedback::MoveCursorOverMap) => {
            ("Move the cursor over the map".to_owned(), String::new())
        }
        Some(BuildingPlacementFeedback::Valid) => ("Valid placement".to_owned(), String::new()),
        Some(BuildingPlacementFeedback::Invalid(error)) => {
            (building_error_text(error).to_owned(), String::new())
        }
        None => {
            if let Some(road) = &status.road {
                if road.cell_count == 0 {
                    return (
                        "Drag across the map to plan a road".to_owned(),
                        String::new(),
                    );
                }
                let cost = format_cost(road.aggregate_cost);
                if road.errors.is_empty() {
                    return (
                        format!("{} tile(s) · Cost: {cost} · Valid stroke", road.cell_count),
                        String::new(),
                    );
                }
                let first = road.errors[0].1.label();
                let remaining = road.errors.len().saturating_sub(1);
                let summary = if remaining == 0 {
                    format!("{} invalid cell(s) · {first}", road.invalid_cell_count)
                } else {
                    format!(
                        "{} invalid cell(s) · {first} · +{remaining} more",
                        road.invalid_cell_count
                    )
                };
                let details = road
                    .errors
                    .iter()
                    .map(|(coord, error)| {
                        format!("({}, {}): {}", coord.x(), coord.y(), error.label())
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                return (summary, details);
            }
            ("Right-click or Escape to cancel".to_owned(), String::new())
        }
    }
}

fn building_error_text(error: BuildingPlacementError) -> &'static str {
    match error {
        BuildingPlacementError::OutOfBounds => "Outside the map boundary",
        BuildingPlacementError::OverlapsBuilding => "Overlaps a building or blueprint",
        BuildingPlacementError::InvalidTerrain => "Cannot be built on this terrain",
        BuildingPlacementError::BlockedByResourceNode => "Blocked by a resource node",
        BuildingPlacementError::BlockedByRoad => "Blocked by a road or road blueprint",
        BuildingPlacementError::FieldRequiresFarm => "A Field requires a selected Farm",
        BuildingPlacementError::TreePlotRequiresLodge => {
            "A Tree Plot requires a selected Forester's Lodge"
        }
    }
}

fn format_cost(cost: ResourceAmounts) -> String {
    let parts = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let amount = cost.get(kind);
            (amount > 0).then(|| format!("{amount} {}", kind.label()))
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "No materials".to_owned()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn categories_have_the_specified_order() {
        assert_eq!(
            ConstructionCategory::ALL.map(ConstructionCategory::label),
            [
                "Storage",
                "Production",
                "Processing",
                "Housing",
                "Civic",
                "Roads"
            ]
        );
    }

    #[test]
    fn catalog_contains_every_global_tool_exactly_once() {
        let tools = ConstructionCategory::ALL
            .into_iter()
            .flat_map(cards_for)
            .map(|card| card.tool)
            .collect::<Vec<_>>();
        let unique = tools.iter().copied().collect::<HashSet<_>>();

        assert_eq!(tools.len(), 14);
        assert_eq!(unique.len(), tools.len());
        for kind in BuildingKind::ALL {
            let expected = !matches!(kind, BuildingKind::Field | BuildingKind::TreePlot);
            assert_eq!(unique.contains(&ConstructionTool::Building(kind)), expected);
        }
        for tier in RoadTier::ALL {
            assert!(unique.contains(&ConstructionTool::Road(tier)));
        }
    }

    #[test]
    fn every_card_has_player_facing_copy_and_cost_scope() {
        for category in ConstructionCategory::ALL {
            for card in cards_for(category) {
                assert!(!card.description.is_empty());
                assert!(!card_button_text(*card).is_empty());
                assert!(card_tooltip(*card).contains(card.description));
            }
        }
    }

    #[test]
    fn building_validation_errors_have_specific_messages() {
        let errors = [
            BuildingPlacementError::OutOfBounds,
            BuildingPlacementError::OverlapsBuilding,
            BuildingPlacementError::InvalidTerrain,
            BuildingPlacementError::BlockedByResourceNode,
            BuildingPlacementError::BlockedByRoad,
            BuildingPlacementError::FieldRequiresFarm,
            BuildingPlacementError::TreePlotRequiresLodge,
        ];
        for error in errors {
            assert!(!building_error_text(error).is_empty());
        }
    }

    #[test]
    fn road_errors_remain_available_in_status_tooltip() {
        let status = ConstructionPlacementStatus {
            active_tool: Some(ConstructionTool::Road(RoadTier::DirtPath)),
            building_feedback: None,
            road: Some(crate::world::game_world::RoadPlacementStatus {
                active_tier: Some(RoadTier::DirtPath),
                cell_count: 2,
                invalid_cell_count: 2,
                aggregate_cost: ResourceAmounts::zero(),
                errors: vec![
                    (
                        game_engine::grid::CellCoord::new(1, 2),
                        game_engine::roads::RoadPlacementError::InvalidTerrain,
                    ),
                    (
                        game_engine::grid::CellCoord::new(2, 2),
                        game_engine::roads::RoadPlacementError::BlockedByResourceNode,
                    ),
                ],
            }),
        };

        let (summary, tooltip) = placement_status_text(&status);

        assert!(summary.contains("2 invalid cell(s)"));
        assert!(summary.contains("+1 more"));
        assert!(tooltip.contains("(1, 2)"));
        assert!(tooltip.contains("(2, 2)"));
    }
}
