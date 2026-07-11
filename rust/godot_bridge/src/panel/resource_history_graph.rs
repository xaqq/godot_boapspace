use godot::classes::{Control, IControl, InputEvent, InputEventMouseMotion};
use godot::prelude::*;

const LEFT_MARGIN: f32 = 64.0;
const TOP_MARGIN: f32 = 16.0;
const RIGHT_MARGIN: f32 = 16.0;
const BOTTOM_MARGIN: f32 = 32.0;
const POINT_RADIUS: f32 = 4.0;
const LIVE_POINT_RADIUS: f32 = 6.0;
const HOVER_RADIUS: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ResourceHistoryRange {
    #[default]
    Days30,
    Days365,
    All,
}

impl ResourceHistoryRange {
    const fn lookback_days(self) -> Option<u64> {
        match self {
            Self::Days30 => Some(30),
            Self::Days365 => Some(365),
            Self::All => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResourceGraphSample {
    pub(crate) day: u64,
    pub(crate) quantity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GraphHistoryPoint {
    day: u64,
    quantity: u64,
    is_live: bool,
}

impl GraphHistoryPoint {
    pub(crate) const fn persisted(day: u64, quantity: u64) -> Self {
        Self {
            day,
            quantity,
            is_live: false,
        }
    }

    pub(crate) const fn live(day: u64, quantity: u64) -> Self {
        Self {
            day,
            quantity,
            is_live: true,
        }
    }
}

impl ResourceGraphSample {
    pub(crate) const fn new(day: u64, quantity: u64) -> Self {
        Self { day, quantity }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayPoint {
    sample: ResourceGraphSample,
    is_live: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PlotArea {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl PlotArea {
    fn from_size(size: Vector2) -> Self {
        Self {
            left: LEFT_MARGIN.min(size.x),
            top: TOP_MARGIN.min(size.y),
            right: (size.x - RIGHT_MARGIN).max(LEFT_MARGIN),
            bottom: (size.y - BOTTOM_MARGIN).max(TOP_MARGIN),
        }
    }

    fn width(self) -> f32 {
        (self.right - self.left).max(0.0)
    }

    fn height(self) -> f32 {
        (self.bottom - self.top).max(0.0)
    }
}

#[derive(GodotClass)]
#[class(base = Control)]
pub(crate) struct ResourceHistoryGraph {
    completed_samples: Vec<ResourceGraphSample>,
    live_sample: ResourceGraphSample,
    range: ResourceHistoryRange,
    hovered_point: Option<DisplayPoint>,
    last_size: Vector2,
    base: Base<Control>,
}

#[godot_api]
impl IControl for ResourceHistoryGraph {
    fn init(base: Base<Control>) -> Self {
        Self {
            completed_samples: Vec::new(),
            live_sample: ResourceGraphSample::new(0, 0),
            range: ResourceHistoryRange::default(),
            hovered_point: None,
            last_size: Vector2::ZERO,
            base,
        }
    }

    fn ready(&mut self) {
        self.last_size = self.base().get_size();
        self.base_mut().set_process(true);
        self.base_mut().queue_redraw();
    }

    fn process(&mut self, _delta: f64) {
        let size = self.base().get_size();
        if size != self.last_size {
            self.last_size = size;
            self.base_mut().queue_redraw();
        }
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        let Ok(mouse_motion) = event.try_cast::<InputEventMouseMotion>() else {
            return;
        };
        let hovered = self.point_near(mouse_motion.get_position());
        if hovered != self.hovered_point {
            self.hovered_point = hovered;
            self.base_mut().queue_redraw();
        }
    }

    fn get_tooltip(&self, at_position: Vector2) -> GString {
        let text = self
            .point_near(at_position)
            .map(point_tooltip)
            .unwrap_or_default();
        GString::from(text.as_str())
    }

    fn draw(&mut self) {
        let size = self.base().get_size();
        let plot_area = PlotArea::from_size(size);
        if plot_area.width() <= 0.0 || plot_area.height() <= 0.0 {
            return;
        }

        let points = self.display_points();
        let bounds = graph_bounds(&points);
        let screen_points = points
            .iter()
            .map(|point| point_position(*point, bounds, plot_area))
            .collect::<Vec<_>>();

        let axis_color = Color::from_rgb(0.55, 0.58, 0.62);
        let line_color = Color::from_rgb(0.25, 0.72, 0.95);
        let sample_color = Color::from_rgb(0.35, 0.82, 1.0);
        let live_color = Color::from_rgb(1.0, 0.72, 0.18);
        let hover_color = Color::from_rgb(1.0, 1.0, 1.0);

        let font = self.base().get_theme_default_font();
        let font_size = self.base().get_theme_default_font_size();
        let hovered_point = self.hovered_point;
        let mut base = self.base_mut();
        base.draw_line(
            Vector2::new(plot_area.left, plot_area.top),
            Vector2::new(plot_area.left, plot_area.bottom),
            axis_color,
        );
        base.draw_line(
            Vector2::new(plot_area.left, plot_area.bottom),
            Vector2::new(plot_area.right, plot_area.bottom),
            axis_color,
        );

        if screen_points.len() >= 2 {
            let polyline = PackedVector2Array::from(screen_points.clone());
            base.draw_polyline_ex(&polyline, line_color)
                .width(2.0)
                .antialiased(true)
                .done();
        }

        for (point, position) in points.iter().zip(&screen_points) {
            let (radius, color) = if point.is_live {
                (LIVE_POINT_RADIUS, live_color)
            } else {
                (POINT_RADIUS, sample_color)
            };
            if point.is_live {
                // A ring remains distinguishable when the live value exactly
                // overlaps today's persisted sample.
                base.draw_circle_ex(*position, radius, color)
                    .filled(false)
                    .width(3.0)
                    .antialiased(true)
                    .done();
            } else {
                base.draw_circle_ex(*position, radius, color)
                    .antialiased(true)
                    .done();
            }
            if Some(*point) == hovered_point {
                base.draw_circle_ex(*position, radius + 3.0, hover_color)
                    .filled(false)
                    .width(2.0)
                    .antialiased(true)
                    .done();
            }
        }

        if let Some(font) = font {
            let text_color = Color::from_rgb(0.82, 0.84, 0.87);
            base.draw_string_ex(&font, Vector2::new(4.0, plot_area.bottom + 4.0), "0")
                .font_size(font_size)
                .modulate(text_color)
                .done();
            base.draw_string_ex(
                &font,
                Vector2::new(4.0, plot_area.top + font_size as f32),
                bounds.max_quantity.to_string().as_str(),
            )
            .font_size(font_size)
            .modulate(text_color)
            .done();
            base.draw_string_ex(
                &font,
                Vector2::new(plot_area.left, size.y - 4.0),
                format!("Day {}", bounds.min_day).as_str(),
            )
            .font_size(font_size)
            .modulate(text_color)
            .done();
            base.draw_string_ex(
                &font,
                Vector2::new((plot_area.right - 80.0).max(plot_area.left), size.y - 4.0),
                format!("Day {}", bounds.max_day).as_str(),
            )
            .font_size(font_size)
            .modulate(text_color)
            .done();
        }
    }
}

impl ResourceHistoryGraph {
    /// Updates the completed history and live point supplied by the resource panel.
    ///
    /// `points` may be in any order. If it contains no live point, the graph still
    /// renders a live zero point for `current_day`.
    pub(crate) fn set_history(&mut self, current_day: u64, points: Vec<GraphHistoryPoint>) {
        let live_sample = points
            .iter()
            .rev()
            .find(|point| point.is_live)
            .map(|point| ResourceGraphSample::new(point.day, point.quantity))
            .unwrap_or_else(|| ResourceGraphSample::new(current_day, 0));
        let completed_samples = points
            .into_iter()
            .filter(|point| !point.is_live)
            .map(|point| ResourceGraphSample::new(point.day, point.quantity))
            .collect();
        self.set_data(completed_samples, live_sample);
    }

    pub(crate) fn show_last_days(&mut self, days: u64) {
        let range = match days {
            365 => ResourceHistoryRange::Days365,
            _ => ResourceHistoryRange::Days30,
        };
        self.set_range(range);
    }

    pub(crate) fn show_all_days(&mut self) {
        self.set_range(ResourceHistoryRange::All);
    }

    pub(crate) fn set_data(
        &mut self,
        mut completed_samples: Vec<ResourceGraphSample>,
        live_sample: ResourceGraphSample,
    ) {
        completed_samples.sort_unstable_by_key(|sample| sample.day);
        if self.completed_samples == completed_samples && self.live_sample == live_sample {
            return;
        }

        self.completed_samples = completed_samples;
        self.live_sample = live_sample;
        self.hovered_point = None;
        self.base_mut().queue_redraw();
    }

    pub(crate) fn set_range(&mut self, range: ResourceHistoryRange) {
        if self.range == range {
            return;
        }
        self.range = range;
        self.hovered_point = None;
        self.base_mut().queue_redraw();
    }

    fn display_points(&self) -> Vec<DisplayPoint> {
        display_points(&self.completed_samples, self.live_sample, self.range)
    }

    fn point_near(&self, position: Vector2) -> Option<DisplayPoint> {
        let plot_area = PlotArea::from_size(self.base().get_size());
        nearest_point(&self.display_points(), plot_area, position, HOVER_RADIUS)
    }
}

fn display_points(
    completed_samples: &[ResourceGraphSample],
    live_sample: ResourceGraphSample,
    range: ResourceHistoryRange,
) -> Vec<DisplayPoint> {
    let first_day = range
        .lookback_days()
        .map(|days| live_sample.day.saturating_sub(days));
    let mut points = completed_samples
        .iter()
        .copied()
        .filter(|sample| first_day.is_none_or(|day| sample.day >= day))
        .map(|sample| DisplayPoint {
            sample,
            is_live: false,
        })
        .collect::<Vec<_>>();
    points.push(DisplayPoint {
        sample: live_sample,
        is_live: true,
    });
    points
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GraphBounds {
    min_day: u64,
    max_day: u64,
    max_quantity: u64,
}

fn graph_bounds(points: &[DisplayPoint]) -> GraphBounds {
    GraphBounds {
        min_day: points
            .iter()
            .map(|point| point.sample.day)
            .min()
            .unwrap_or(0),
        max_day: points
            .iter()
            .map(|point| point.sample.day)
            .max()
            .unwrap_or(0),
        max_quantity: points
            .iter()
            .map(|point| point.sample.quantity)
            .max()
            .unwrap_or(0)
            .max(1),
    }
}

fn point_position(point: DisplayPoint, bounds: GraphBounds, area: PlotArea) -> Vector2 {
    let x = if bounds.min_day == bounds.max_day {
        area.left + area.width() / 2.0
    } else {
        let day_offset = point.sample.day.saturating_sub(bounds.min_day) as f64;
        let day_span = (bounds.max_day - bounds.min_day) as f64;
        area.left + (day_offset / day_span) as f32 * area.width()
    };
    let y_fraction = point.sample.quantity as f64 / bounds.max_quantity as f64;
    let y = area.bottom - y_fraction as f32 * area.height();
    Vector2::new(x, y)
}

fn nearest_point(
    points: &[DisplayPoint],
    area: PlotArea,
    cursor: Vector2,
    radius: f32,
) -> Option<DisplayPoint> {
    let bounds = graph_bounds(points);
    points
        .iter()
        .copied()
        .filter_map(|point| {
            let distance = point_position(point, bounds, area).distance_to(cursor);
            (distance <= radius).then_some((distance, point))
        })
        .min_by(
            |(left_distance, left_point), (right_distance, right_point)| {
                left_distance
                    .total_cmp(right_distance)
                    .then_with(|| right_point.is_live.cmp(&left_point.is_live))
            },
        )
        .map(|(_, point)| point)
}

fn point_tooltip(point: DisplayPoint) -> String {
    if point.is_live {
        format!("Now: {}", point.sample.quantity)
    } else {
        format!("Day {}: {}", point.sample.day, point.sample.quantity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(day: u64, quantity: u64) -> ResourceGraphSample {
        ResourceGraphSample::new(day, quantity)
    }

    #[test]
    fn range_filters_completed_samples_and_always_includes_live_point() {
        let completed = [sample(1, 10), sample(70, 20), sample(99, 30)];
        let live = sample(100, 40);

        let points = display_points(&completed, live, ResourceHistoryRange::Days30);

        assert_eq!(
            points,
            vec![
                DisplayPoint {
                    sample: sample(70, 20),
                    is_live: false
                },
                DisplayPoint {
                    sample: sample(99, 30),
                    is_live: false
                },
                DisplayPoint {
                    sample: live,
                    is_live: true
                }
            ]
        );
    }

    #[test]
    fn all_range_keeps_every_completed_sample() {
        let completed = [sample(1, 10), sample(500, 20)];
        let points = display_points(&completed, sample(501, 30), ResourceHistoryRange::All);

        assert_eq!(points.len(), 3);
        assert_eq!(points[0].sample.day, 1);
        assert!(points.last().unwrap().is_live);
    }

    #[test]
    fn day_range_saturates_for_young_surfaces() {
        let completed = [sample(0, 10), sample(3, 20)];
        let points = display_points(&completed, sample(4, 30), ResourceHistoryRange::Days365);

        assert_eq!(points.len(), 3);
    }

    #[test]
    fn graph_coordinates_span_plot_area_and_zero_baseline() {
        let points = [
            DisplayPoint {
                sample: sample(10, 0),
                is_live: false,
            },
            DisplayPoint {
                sample: sample(20, 100),
                is_live: true,
            },
        ];
        let bounds = graph_bounds(&points);
        let area = PlotArea {
            left: 10.0,
            top: 20.0,
            right: 110.0,
            bottom: 220.0,
        };

        assert_eq!(
            point_position(points[0], bounds, area),
            Vector2::new(10.0, 220.0)
        );
        assert_eq!(
            point_position(points[1], bounds, area),
            Vector2::new(110.0, 20.0)
        );
    }

    #[test]
    fn one_day_is_centered_instead_of_dividing_by_zero() {
        let point = DisplayPoint {
            sample: sample(10, 5),
            is_live: true,
        };
        let area = PlotArea {
            left: 0.0,
            top: 0.0,
            right: 100.0,
            bottom: 100.0,
        };

        assert_eq!(point_position(point, graph_bounds(&[point]), area).x, 50.0);
    }

    #[test]
    fn nearest_point_respects_hit_radius() {
        let point = DisplayPoint {
            sample: sample(10, 5),
            is_live: true,
        };
        let area = PlotArea {
            left: 0.0,
            top: 0.0,
            right: 100.0,
            bottom: 100.0,
        };

        assert_eq!(
            nearest_point(&[point], area, Vector2::new(50.0, 0.0), 1.0),
            Some(point)
        );
        assert_eq!(
            nearest_point(&[point], area, Vector2::new(80.0, 0.0), 1.0),
            None
        );
    }

    #[test]
    fn live_point_wins_hover_ties_with_an_overlapping_sample() {
        let persisted = DisplayPoint {
            sample: sample(10, 5),
            is_live: false,
        };
        let live = DisplayPoint {
            sample: sample(10, 5),
            is_live: true,
        };
        let area = PlotArea {
            left: 0.0,
            top: 0.0,
            right: 100.0,
            bottom: 100.0,
        };

        assert_eq!(
            nearest_point(&[persisted, live], area, Vector2::new(50.0, 0.0), 1.0),
            Some(live)
        );
    }

    #[test]
    fn tooltips_identify_completed_and_live_points() {
        assert_eq!(
            point_tooltip(DisplayPoint {
                sample: sample(12, 34),
                is_live: false,
            }),
            "Day 12: 34"
        );
        assert_eq!(
            point_tooltip(DisplayPoint {
                sample: sample(12, 56),
                is_live: true,
            }),
            "Now: 56"
        );
    }
}
