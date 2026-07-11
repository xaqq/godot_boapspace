use super::resource_history_graph::{GraphHistoryPoint, ResourceHistoryGraph};
use crate::assets::{load_texture, resource_asset_path};
use crate::world::game_world::GameWorld;
use game_engine::resources::{resource_overview, ResourceHistory, ResourceKind};
use godot::classes::{control, Button, GridContainer, IPanelContainer, Label, PanelContainer};
use godot::obj::{NewAlloc, OnEditor};
use godot::prelude::*;

const LOOKBACKS: [u64; 4] = [1, 7, 30, 365];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeltaState {
    Positive,
    Negative,
    Zero,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourcePanelRowView {
    kind: ResourceKind,
    now: u64,
    committed: u64,
    changes: [Option<i128>; LOOKBACKS.len()],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourcePanelView {
    surface_index: i32,
    day: u64,
    rows: Vec<ResourcePanelRowView>,
    history: Vec<GraphHistoryPoint>,
}

struct ResourcePanelRowControls {
    kind: ResourceKind,
    button: Gd<Button>,
    now: Gd<Label>,
    committed: Gd<Label>,
    changes: [Gd<Label>; LOOKBACKS.len()],
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct ResourcePanel {
    #[export]
    close_button: OnEditor<Gd<Button>>,

    #[export]
    toggle_button: OnEditor<Gd<Button>>,

    #[export]
    row_container: OnEditor<Gd<GridContainer>>,

    #[export]
    empty_state_label: OnEditor<Gd<Label>>,

    #[export]
    graph: OnEditor<Gd<ResourceHistoryGraph>>,

    #[export]
    range_30d_button: OnEditor<Gd<Button>>,

    #[export]
    range_365d_button: OnEditor<Gd<Button>>,

    #[export]
    range_all_button: OnEditor<Gd<Button>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    rows: Vec<ResourcePanelRowControls>,
    selected_resource: Option<ResourceKind>,
    cached_view: Option<ResourcePanelView>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for ResourcePanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            close_button: OnEditor::default(),
            toggle_button: OnEditor::default(),
            row_container: OnEditor::default(),
            empty_state_label: OnEditor::default(),
            graph: OnEditor::default(),
            range_30d_button: OnEditor::default(),
            range_365d_button: OnEditor::default(),
            range_all_button: OnEditor::default(),
            game_world: OnEditor::default(),
            rows: Vec::new(),
            selected_resource: None,
            cached_view: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.close_button
            .clone()
            .signals()
            .pressed()
            .connect_other(self, Self::hide_panel);
        self.toggle_button
            .clone()
            .signals()
            .pressed()
            .connect_other(self, Self::toggle_panel);
        self.range_30d_button
            .clone()
            .signals()
            .pressed()
            .connect_other(self, Self::select_range_30d);
        self.range_365d_button
            .clone()
            .signals()
            .pressed()
            .connect_other(self, Self::select_range_365d);
        self.range_all_button
            .clone()
            .signals()
            .pressed()
            .connect_other(self, Self::select_range_all);

        self.build_rows();
        self.show_empty_state();
        self.select_range_30d();
        self.base_mut().hide();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible() {
            self.refresh();
        }
    }
}

impl ResourcePanel {
    fn build_rows(&mut self) {
        let mut container = self.row_container.clone();
        for kind in ResourceKind::ALL {
            let mut button = Button::new_alloc();
            button.set_text(kind.label());
            button.set_tooltip_text(format!("{}\n{}", kind.label(), kind.description()).as_str());
            button.set_toggle_mode(true);
            button.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
            if let Some(texture) = load_texture(resource_asset_path(kind), "ResourcePanel") {
                button.set_button_icon(&texture);
                button.set_expand_icon(true);
            }
            button
                .signals()
                .pressed()
                .connect_other(self, move |panel| panel.select_resource(kind));
            container.add_child(&button);

            let mut labels = Vec::with_capacity(6);
            for _ in 0..6 {
                let mut label = Label::new_alloc();
                label.set_text("0");
                label.set_horizontal_alignment(godot::global::HorizontalAlignment::RIGHT);
                label.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
                container.add_child(&label);
                labels.push(label);
            }

            self.rows.push(ResourcePanelRowControls {
                kind,
                button,
                now: labels.remove(0),
                committed: labels.remove(0),
                changes: labels.try_into().expect("four delta labels were created"),
            });
        }
    }

    fn hide_panel(&mut self) {
        self.base_mut().hide();
    }

    fn toggle_panel(&mut self) {
        if self.base().is_visible() {
            self.hide_panel();
        } else {
            self.base_mut().show();
            self.refresh();
        }
    }

    fn select_resource(&mut self, kind: ResourceKind) {
        self.selected_resource = Some(kind);
        for row in &mut self.rows {
            row.button.set_pressed_no_signal(row.kind == kind);
        }
        self.empty_state_label.clone().hide();
        self.graph.clone().show();
        self.cached_view = None;
        self.refresh();
    }

    fn select_range_30d(&mut self) {
        self.set_range_buttons(true, false, false);
        self.graph.bind_mut().show_last_days(30);
    }

    fn select_range_365d(&mut self) {
        self.set_range_buttons(false, true, false);
        self.graph.bind_mut().show_last_days(365);
    }

    fn select_range_all(&mut self) {
        self.set_range_buttons(false, false, true);
        self.graph.bind_mut().show_all_days();
    }

    fn set_range_buttons(&mut self, days_30: bool, days_365: bool, all: bool) {
        self.range_30d_button.clone().set_pressed_no_signal(days_30);
        self.range_365d_button
            .clone()
            .set_pressed_no_signal(days_365);
        self.range_all_button.clone().set_pressed_no_signal(all);
    }

    fn show_empty_state(&mut self) {
        self.empty_state_label.clone().show();
        self.graph.clone().hide();
    }

    fn refresh(&mut self) {
        let selected = self.selected_resource;
        let surface_index = self.game_world.bind().active_surface_index();
        let view = {
            let game_world = self.game_world.bind();
            game_world.with_rendered_surface_world(|world| {
                build_panel_view(world, surface_index, selected)
            })
        };
        if self.cached_view.as_ref() == Some(&view) {
            return;
        }

        for (controls, row) in self.rows.iter_mut().zip(&view.rows) {
            controls.now.set_text(row.now.to_string().as_str());
            controls
                .committed
                .set_text(row.committed.to_string().as_str());
            for (label, change) in controls.changes.iter_mut().zip(row.changes) {
                let (text, state) = delta_presentation(change);
                label.set_text(text.as_str());
                label.set_modulate(delta_color(state));
            }
        }

        if selected.is_some() {
            self.graph
                .bind_mut()
                .set_history(view.day, view.history.clone());
        }
        self.cached_view = Some(view);
    }
}

fn build_panel_view(
    world: &bevy_ecs::world::World,
    surface_index: i32,
    selected: Option<ResourceKind>,
) -> ResourcePanelView {
    let overview = resource_overview(world);
    let history = world.resource::<ResourceHistory>();
    let day = world.resource::<game_engine::npcs::WorldDateTime>().day();
    let rows = ResourceKind::ALL
        .into_iter()
        .map(|kind| {
            let now = overview.usable().get(kind);
            ResourcePanelRowView {
                kind,
                now,
                committed: overview.committed().get(kind),
                changes: LOOKBACKS.map(|days| history.change_since(day, days, kind, now)),
            }
        })
        .collect();
    let history = selected
        .map(|kind| {
            let mut points = history
                .samples()
                .iter()
                .map(|sample| GraphHistoryPoint::persisted(sample.day(), sample.quantity(kind)))
                .collect::<Vec<_>>();
            points.push(GraphHistoryPoint::live(day, overview.usable().get(kind)));
            points
        })
        .unwrap_or_default();

    ResourcePanelView {
        surface_index,
        day,
        rows,
        history,
    }
}

fn delta_presentation(change: Option<i128>) -> (String, DeltaState) {
    match change {
        Some(value) if value > 0 => (format!("+{value}"), DeltaState::Positive),
        Some(value) if value < 0 => (value.to_string(), DeltaState::Negative),
        Some(_) => ("0".to_owned(), DeltaState::Zero),
        None => ("—".to_owned(), DeltaState::Unavailable),
    }
}

fn delta_color(state: DeltaState) -> Color {
    match state {
        DeltaState::Positive => Color::from_rgb(0.35, 0.9, 0.45),
        DeltaState::Negative => Color::from_rgb(1.0, 0.4, 0.4),
        DeltaState::Zero => Color::from_rgb(0.9, 0.9, 0.9),
        DeltaState::Unavailable => Color::from_rgb(0.55, 0.55, 0.55),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_text_and_state_include_sign_and_unavailable_marker() {
        assert_eq!(
            delta_presentation(Some(4)),
            ("+4".into(), DeltaState::Positive)
        );
        assert_eq!(
            delta_presentation(Some(-3)),
            ("-3".into(), DeltaState::Negative)
        );
        assert_eq!(delta_presentation(Some(0)), ("0".into(), DeltaState::Zero));
        assert_eq!(
            delta_presentation(None),
            ("—".into(), DeltaState::Unavailable)
        );
    }

    #[test]
    fn row_order_follows_resource_kind_all() {
        let mut world = bevy_ecs::world::World::new();
        world.insert_resource(game_engine::npcs::WorldDateTime::from_day(10));
        world.insert_resource(ResourceHistory::new(10, Default::default()));
        let view = build_panel_view(&world, 0, None);
        assert_eq!(
            view.rows.iter().map(|row| row.kind).collect::<Vec<_>>(),
            ResourceKind::ALL
        );
    }
}
