use super::housing_panel::housing_overview;
use super::resource_panel::{resource_delta_text, RESOURCE_LOOKBACKS};
use crate::assets::{load_texture, resource_asset_path};
use crate::world::game_world::{GameWorld, RendererMode};
use game_engine::resources::{ResourceHistory, ResourceKind, ResourceOverview};
use godot::classes::{
    canvas_item::TextureFilter, control, texture_rect, Button, HBoxContainer, IHBoxContainer,
    Label, TextureRect,
};
use godot::global::HorizontalAlignment;
use godot::obj::{NewAlloc, OnEditor};
use godot::prelude::*;

const SUMMARY_REFRESH_INTERVAL_SECONDS: f64 = 0.25;
const SUMMARY_ICON_SIZE: f32 = 20.0;
const SUMMARY_FONT_SIZE: i32 = 14;
const HOMELESS_WARNING_COLOR: Color = Color::from_rgb(1.0, 0.4, 0.4);
const HOMELESS_NEUTRAL_COLOR: Color = Color::from_rgb(1.0, 1.0, 1.0);
const TWO_D_RENDERER_TOOLTIP: &str = "Use the 2D world renderer.";
const THREE_D_RENDERER_TOOLTIP: &str = "Experimental 3D world renderer.";
const RENDERER_UNAVAILABLE_FALLBACK: &str = "This renderer is currently unavailable.";
const COMPACT_QUANTITY_UNITS: [(u64, &str); 6] = [
    (1_000, "K"),
    (1_000_000, "M"),
    (1_000_000_000, "B"),
    (1_000_000_000_000, "T"),
    (1_000_000_000_000_000, "Q"),
    (1_000_000_000_000_000_000, "E"),
];

struct HeaderResourceControls {
    kind: ResourceKind,
    item: Gd<HBoxContainer>,
    amount_label: Gd<Label>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeaderResourceView {
    kind: ResourceKind,
    usable: u64,
    committed: u64,
    changes: [Option<i128>; RESOURCE_LOOKBACKS.len()],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeaderHousingView {
    occupied: usize,
    capacity: usize,
    homeless: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeaderView {
    surface_index: i32,
    resources: Vec<HeaderResourceView>,
    housing: HeaderHousingView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RendererButtonView {
    disabled: bool,
    tooltip: String,
}

#[derive(GodotClass)]
#[class(base = HBoxContainer)]
pub(crate) struct SimulationHeaderBar {
    #[export]
    play_pause_button: OnEditor<Gd<Button>>,

    #[export]
    speed_1x_button: OnEditor<Gd<Button>>,

    #[export]
    speed_2x_button: OnEditor<Gd<Button>>,

    #[export]
    speed_4x_button: OnEditor<Gd<Button>>,

    #[export]
    speed_50x_button: OnEditor<Gd<Button>>,

    #[export]
    speed_100x_button: OnEditor<Gd<Button>>,

    #[export]
    renderer_2d_button: OnEditor<Gd<Button>>,

    #[export]
    renderer_3d_button: OnEditor<Gd<Button>>,

    #[export]
    datetime_label: OnEditor<Gd<Label>>,

    #[export]
    resource_summary_container: OnEditor<Gd<HBoxContainer>>,

    #[export]
    homeless_summary: OnEditor<Gd<HBoxContainer>>,

    #[export]
    homeless_icon: OnEditor<Gd<TextureRect>>,

    #[export]
    homeless_count_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    resource_controls: Vec<HeaderResourceControls>,
    cached_view: Option<HeaderView>,
    summary_refresh_elapsed: f64,
    base: Base<HBoxContainer>,
}

#[godot_api]
impl IHBoxContainer for SimulationHeaderBar {
    fn init(base: Base<HBoxContainer>) -> Self {
        Self {
            play_pause_button: OnEditor::default(),
            speed_1x_button: OnEditor::default(),
            speed_2x_button: OnEditor::default(),
            speed_4x_button: OnEditor::default(),
            speed_50x_button: OnEditor::default(),
            speed_100x_button: OnEditor::default(),
            renderer_2d_button: OnEditor::default(),
            renderer_3d_button: OnEditor::default(),
            datetime_label: OnEditor::default(),
            resource_summary_container: OnEditor::default(),
            homeless_summary: OnEditor::default(),
            homeless_icon: OnEditor::default(),
            homeless_count_label: OnEditor::default(),
            game_world: OnEditor::default(),
            resource_controls: Vec::new(),
            cached_view: None,
            summary_refresh_elapsed: 0.0,
            base,
        }
    }

    fn ready(&mut self) {
        let play_pause_button = self.play_pause_button.clone();
        let speed_1x_button = self.speed_1x_button.clone();
        let speed_2x_button = self.speed_2x_button.clone();
        let speed_4x_button = self.speed_4x_button.clone();
        let speed_50x_button = self.speed_50x_button.clone();
        let speed_100x_button = self.speed_100x_button.clone();
        let renderer_2d_button = self.renderer_2d_button.clone();
        let renderer_3d_button = self.renderer_3d_button.clone();
        let game_world = self.game_world.clone();

        play_pause_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.toggle_simulation_playing();
            },
        );

        speed_1x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(1);
            },
        );

        speed_2x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(2);
            },
        );

        speed_4x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(4);
            },
        );

        speed_50x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(50);
            },
        );

        speed_100x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(100);
            },
        );

        renderer_2d_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_renderer_mode(RendererMode::TwoD);
            },
        );

        renderer_3d_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_renderer_mode(RendererMode::ThreeD);
            },
        );

        self.build_resource_controls();
        self.refresh_controls();
        self.refresh_summary();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, delta: f64) {
        self.refresh_controls();
        self.refresh_summary_if_due(delta);
    }
}

impl SimulationHeaderBar {
    fn build_resource_controls(&mut self) {
        let mut container = self.resource_summary_container.clone();
        for kind in ResourceKind::ALL {
            let mut item = HBoxContainer::new_alloc();
            item.add_theme_constant_override("separation", 1);
            item.set_mouse_filter(control::MouseFilter::STOP);
            item.set_tooltip_text(kind.label());

            let mut icon = TextureRect::new_alloc();
            icon.set_custom_minimum_size(Vector2::new(SUMMARY_ICON_SIZE, SUMMARY_ICON_SIZE));
            icon.set_expand_mode(texture_rect::ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_texture_filter(TextureFilter::NEAREST);
            icon.set_mouse_filter(control::MouseFilter::IGNORE);
            if let Some(texture) = load_texture(resource_asset_path(kind), "SimulationHeaderBar") {
                icon.set_texture(&texture);
            }

            let mut amount_label = Label::new_alloc();
            amount_label.set_text("0");
            amount_label.set_horizontal_alignment(HorizontalAlignment::RIGHT);
            amount_label.add_theme_font_size_override("font_size", SUMMARY_FONT_SIZE);
            amount_label.set_mouse_filter(control::MouseFilter::IGNORE);

            item.add_child(&icon);
            item.add_child(&amount_label);
            container.add_child(&item);
            self.resource_controls.push(HeaderResourceControls {
                kind,
                item,
                amount_label,
            });
        }
    }

    fn refresh_controls(&mut self) {
        let game_world = self.game_world.clone();
        let game_world = game_world.bind();
        let is_playing = game_world.is_simulation_playing();
        let datetime_text = game_world.simulation_datetime_text_string();
        let simulation_speed_multiplier = game_world.simulation_speed_multiplier();
        let active_renderer_mode = game_world.active_renderer_mode();
        let renderer_2d_view = renderer_button_view(
            RendererMode::TwoD,
            active_renderer_mode,
            game_world.renderer_mode_available(RendererMode::TwoD),
            game_world.renderer_mode_unavailable_reason(RendererMode::TwoD),
        );
        let renderer_3d_view = renderer_button_view(
            RendererMode::ThreeD,
            active_renderer_mode,
            game_world.renderer_mode_available(RendererMode::ThreeD),
            game_world.renderer_mode_unavailable_reason(RendererMode::ThreeD),
        );
        drop(game_world);

        let mut play_pause_button = self.play_pause_button.clone();
        play_pause_button.set_text(play_pause_text(is_playing));

        self.refresh_speed_button_states(simulation_speed_multiplier);
        self.refresh_renderer_button_states(renderer_2d_view, renderer_3d_view);

        let mut datetime_label = self.datetime_label.clone();
        datetime_label.set_text(datetime_text.as_str());
    }

    fn refresh_summary_if_due(&mut self, delta: f64) {
        self.summary_refresh_elapsed += delta;
        let surface_index = self.game_world.bind().active_surface_index();
        let surface_changed = self
            .cached_view
            .as_ref()
            .is_none_or(|view| view.surface_index != surface_index);
        if !surface_changed && self.summary_refresh_elapsed < SUMMARY_REFRESH_INTERVAL_SECONDS {
            return;
        }

        self.refresh_summary();
    }

    fn refresh_summary(&mut self) {
        let surface_index = self.game_world.bind().active_surface_index();
        let view = {
            let mut game_world = self.game_world.bind_mut();
            game_world.with_rendered_surface_resource_overview(|overview, world| {
                build_header_view(world, overview, surface_index)
            })
        };
        self.summary_refresh_elapsed = 0.0;
        if self.cached_view.as_ref() == Some(&view) {
            return;
        }

        for (controls, resource) in self.resource_controls.iter_mut().zip(&view.resources) {
            debug_assert_eq!(controls.kind, resource.kind);
            controls
                .amount_label
                .set_text(format_compact_quantity(resource.usable).as_str());
            controls
                .item
                .set_tooltip_text(resource_tooltip_text(resource).as_str());
        }

        let homeless_quantity = u64::try_from(view.housing.homeless).unwrap_or(u64::MAX);
        self.homeless_count_label
            .clone()
            .set_text(format_compact_quantity(homeless_quantity).as_str());
        self.homeless_summary
            .clone()
            .set_tooltip_text(housing_tooltip_text(view.housing).as_str());

        let homeless_color = if homeless_is_warning(view.housing.homeless) {
            HOMELESS_WARNING_COLOR
        } else {
            HOMELESS_NEUTRAL_COLOR
        };
        self.homeless_icon.clone().set_modulate(homeless_color);
        self.homeless_count_label
            .clone()
            .set_modulate(homeless_color);

        self.cached_view = Some(view);
    }

    fn refresh_speed_button_states(&mut self, active_multiplier: i32) {
        let mut speed_1x_button = self.speed_1x_button.clone();
        speed_1x_button.set_disabled(speed_button_disabled(1, active_multiplier));

        let mut speed_2x_button = self.speed_2x_button.clone();
        speed_2x_button.set_disabled(speed_button_disabled(2, active_multiplier));

        let mut speed_4x_button = self.speed_4x_button.clone();
        speed_4x_button.set_disabled(speed_button_disabled(4, active_multiplier));

        let mut speed_50x_button = self.speed_50x_button.clone();
        speed_50x_button.set_disabled(speed_button_disabled(50, active_multiplier));

        let mut speed_100x_button = self.speed_100x_button.clone();
        speed_100x_button.set_disabled(speed_button_disabled(100, active_multiplier));
    }

    fn refresh_renderer_button_states(
        &mut self,
        renderer_2d_view: RendererButtonView,
        renderer_3d_view: RendererButtonView,
    ) {
        let mut renderer_2d_button = self.renderer_2d_button.clone();
        renderer_2d_button.set_disabled(renderer_2d_view.disabled);
        renderer_2d_button.set_tooltip_text(renderer_2d_view.tooltip.as_str());

        let mut renderer_3d_button = self.renderer_3d_button.clone();
        renderer_3d_button.set_disabled(renderer_3d_view.disabled);
        renderer_3d_button.set_tooltip_text(renderer_3d_view.tooltip.as_str());
    }
}

fn build_header_view(
    world: &bevy_ecs::world::World,
    overview: ResourceOverview,
    surface_index: i32,
) -> HeaderView {
    let history = world.resource::<ResourceHistory>();
    let day = world.resource::<game_engine::npcs::WorldDateTime>().day();
    let resources = ResourceKind::ALL
        .into_iter()
        .map(|kind| {
            let usable = overview.usable().get(kind);
            HeaderResourceView {
                kind,
                usable,
                committed: overview.committed().get(kind),
                changes: RESOURCE_LOOKBACKS
                    .map(|days| history.change_since(day, days, kind, usable)),
            }
        })
        .collect();
    let housing = housing_overview(world);

    HeaderView {
        surface_index,
        resources,
        housing: HeaderHousingView {
            occupied: housing.total_occupied(),
            capacity: housing.total_capacity(),
            homeless: housing.homeless(),
        },
    }
}

fn resource_tooltip_text(resource: &HeaderResourceView) -> String {
    let changes = resource.changes.map(resource_delta_text);
    format!(
        "{}\nUsable: {}\nCommitted: {}\n1 day: {}\n7 days: {}\n30 days: {}\n365 days: {}",
        resource.kind.label(),
        resource.usable,
        resource.committed,
        changes[0],
        changes[1],
        changes[2],
        changes[3]
    )
}

fn housing_tooltip_text(housing: HeaderHousingView) -> String {
    format!(
        "Homelessness\nHomeless colonists: {}\nHousing slots: {}/{}",
        housing.homeless, housing.occupied, housing.capacity
    )
}

fn format_compact_quantity(quantity: u64) -> String {
    let Some((unit, suffix)) = COMPACT_QUANTITY_UNITS
        .iter()
        .rev()
        .find(|(unit, _)| quantity >= *unit)
    else {
        return quantity.to_string();
    };

    let whole = quantity / unit;
    if whole >= 10 {
        return format!("{whole}{suffix}");
    }

    let tenths = (quantity % unit) / (unit / 10);
    if tenths == 0 {
        format!("{whole}{suffix}")
    } else {
        format!("{whole}.{tenths}{suffix}")
    }
}

fn homeless_is_warning(homeless: usize) -> bool {
    homeless > 0
}

fn play_pause_text(is_playing: bool) -> &'static str {
    if is_playing {
        "Pause"
    } else {
        "Play"
    }
}

fn speed_button_disabled(button_multiplier: i32, active_multiplier: i32) -> bool {
    button_multiplier == active_multiplier
}

fn renderer_button_view(
    button_mode: RendererMode,
    active_mode: RendererMode,
    available: bool,
    unavailable_reason: Option<&str>,
) -> RendererButtonView {
    let mut tooltip = match button_mode {
        RendererMode::TwoD => TWO_D_RENDERER_TOOLTIP.to_owned(),
        RendererMode::ThreeD => THREE_D_RENDERER_TOOLTIP.to_owned(),
    };
    if !available {
        tooltip.push('\n');
        tooltip.push_str(unavailable_reason.unwrap_or(RENDERER_UNAVAILABLE_FALLBACK));
    }

    RendererButtonView {
        disabled: button_mode == active_mode || !available,
        tooltip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::buildings::{
        BuildingBlueprint, BuildingFootprint, BuildingKind, ConstructionProgress,
        WarehouseInventory,
    };
    use game_engine::components::Npc;
    use game_engine::grid::CellCoord;
    use game_engine::housing::HousingAssignment;
    use game_engine::resources::{resource_overview, ResourceAmounts};

    #[test]
    fn play_pause_text_matches_next_action() {
        assert_eq!(play_pause_text(true), "Pause");
        assert_eq!(play_pause_text(false), "Play");
    }

    #[test]
    fn active_speed_button_is_disabled() {
        assert!(speed_button_disabled(2, 2));
        assert!(!speed_button_disabled(1, 2));
        assert!(!speed_button_disabled(4, 2));
        assert!(!speed_button_disabled(50, 2));
        assert!(!speed_button_disabled(100, 2));
    }

    #[test]
    fn active_renderer_button_is_disabled() {
        let two_d = renderer_button_view(RendererMode::TwoD, RendererMode::TwoD, true, None);
        let three_d = renderer_button_view(RendererMode::ThreeD, RendererMode::ThreeD, true, None);

        assert!(two_d.disabled);
        assert_eq!(two_d.tooltip, TWO_D_RENDERER_TOOLTIP);
        assert!(three_d.disabled);
        assert_eq!(three_d.tooltip, THREE_D_RENDERER_TOOLTIP);
    }

    #[test]
    fn preparing_renderer_is_disabled_with_experimental_tooltip() {
        let view = renderer_button_view(
            RendererMode::ThreeD,
            RendererMode::TwoD,
            false,
            Some("Preparing 3D renderer assets."),
        );

        assert!(view.disabled);
        assert_eq!(
            view.tooltip,
            "Experimental 3D world renderer.\nPreparing 3D renderer assets."
        );
    }

    #[test]
    fn ready_renderer_is_enabled_when_inactive() {
        let view = renderer_button_view(RendererMode::ThreeD, RendererMode::TwoD, true, None);

        assert!(!view.disabled);
        assert_eq!(view.tooltip, THREE_D_RENDERER_TOOLTIP);
    }

    #[test]
    fn failed_renderer_is_disabled_with_failure_reason() {
        let view = renderer_button_view(
            RendererMode::ThreeD,
            RendererMode::TwoD,
            false,
            Some("3D renderer preparation failed."),
        );

        assert!(view.disabled);
        assert_eq!(
            view.tooltip,
            "Experimental 3D world renderer.\n3D renderer preparation failed."
        );
    }

    #[test]
    fn compact_quantity_uses_stable_truncated_suffixes() {
        assert_eq!(format_compact_quantity(0), "0");
        assert_eq!(format_compact_quantity(999), "999");
        assert_eq!(format_compact_quantity(1_000), "1K");
        assert_eq!(format_compact_quantity(1_250), "1.2K");
        assert_eq!(format_compact_quantity(9_999), "9.9K");
        assert_eq!(format_compact_quantity(10_000), "10K");
        assert_eq!(format_compact_quantity(999_999), "999K");
        assert_eq!(format_compact_quantity(1_200_000), "1.2M");
        assert_eq!(format_compact_quantity(u64::MAX), "18E");
    }

    #[test]
    fn tooltip_text_contains_exact_operational_details() {
        let resource = HeaderResourceView {
            kind: ResourceKind::Wood,
            usable: 1_250,
            committed: 5,
            changes: [Some(4), Some(-3), Some(0), None],
        };
        assert_eq!(
            resource_tooltip_text(&resource),
            "Wood\nUsable: 1250\nCommitted: 5\n1 day: +4\n7 days: -3\n30 days: 0\n365 days: —"
        );

        let housing = HeaderHousingView {
            occupied: 7,
            capacity: 10,
            homeless: 2,
        };
        assert_eq!(
            housing_tooltip_text(housing),
            "Homelessness\nHomeless colonists: 2\nHousing slots: 7/10"
        );
        assert!(!homeless_is_warning(0));
        assert!(homeless_is_warning(2));
    }

    #[test]
    fn header_view_uses_resource_history_and_housing_snapshot() {
        let mut world = bevy_ecs::world::World::new();
        let initial = resource_overview(&mut world).usable();
        let mut history = ResourceHistory::new(1, initial);
        for day in [336, 359, 365] {
            assert!(history.record_day(day, initial));
        }
        world.insert_resource(history);
        world.insert_resource(game_engine::npcs::WorldDateTime::from_day(366));
        let mut warehouse = WarehouseInventory::empty();
        assert!(warehouse.add(ResourceKind::Wood, 12));
        world.spawn(warehouse);
        world.spawn((
            BuildingBlueprint {
                kind: BuildingKind::TownHall,
                footprint: BuildingFootprint::new(CellCoord::new(0, 0), 3, 3),
            },
            ConstructionProgress::new(ResourceAmounts::of(ResourceKind::Wood, 5)),
        ));
        world.register_component::<HousingAssignment>();
        world.spawn((Npc,));

        let overview = resource_overview(&mut world);
        let view = build_header_view(&world, overview, 3);

        assert_eq!(view.surface_index, 3);
        assert_eq!(
            view.resources
                .iter()
                .map(|row| row.kind)
                .collect::<Vec<_>>(),
            ResourceKind::ALL
        );
        assert_eq!(view.resources[0].usable, 12);
        assert_eq!(view.resources[0].committed, 5);
        assert_eq!(view.resources[0].changes, [Some(12); 4]);
        assert_eq!(
            view.housing,
            HeaderHousingView {
                occupied: 0,
                capacity: 0,
                homeless: 1,
            }
        );
    }
}
