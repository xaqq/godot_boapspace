use crate::world::game_world::{GameWorld, TickPerformanceSample};
use godot::classes::{control, Engine, IPanelContainer, InputEvent, Label, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;
use std::time::Duration;

const ACTION_PERFORMANCE_INFO_TOGGLE: &str = "performance_info_toggle";
const REFRESH_INTERVAL_SECONDS: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct TickPerformanceReport {
    world_tick_milliseconds: f64,
    fixed_tick_milliseconds: f64,
}

#[derive(Debug, Default)]
struct TickPerformanceAccumulator {
    total_wall_time: Duration,
    world_tick_count: u64,
    fixed_tick_count: u64,
}

impl TickPerformanceAccumulator {
    fn add(&mut self, sample: TickPerformanceSample) {
        if sample.fixed_tick_count == 0 {
            return;
        }

        self.total_wall_time += sample.wall_time;
        self.world_tick_count += 1;
        self.fixed_tick_count += sample.fixed_tick_count;
    }

    fn take_report(&mut self) -> Option<TickPerformanceReport> {
        let report = if self.world_tick_count == 0 || self.fixed_tick_count == 0 {
            None
        } else {
            let total_milliseconds = self.total_wall_time.as_secs_f64() * 1_000.0;
            Some(TickPerformanceReport {
                world_tick_milliseconds: total_milliseconds / self.world_tick_count as f64,
                fixed_tick_milliseconds: total_milliseconds / self.fixed_tick_count as f64,
            })
        };
        *self = Self::default();
        report
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct PerformanceInfo {
    #[export]
    info_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    refresh_elapsed: f64,
    last_sample_sequence: Option<u64>,
    was_playing: bool,
    tick_accumulator: TickPerformanceAccumulator,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for PerformanceInfo {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            info_label: OnEditor::default(),
            game_world: OnEditor::default(),
            refresh_elapsed: 0.0,
            last_sample_sequence: None,
            was_playing: true,
            tick_accumulator: TickPerformanceAccumulator::default(),
            base,
        }
    }

    fn ready(&mut self) {
        self.base_mut()
            .set_mouse_filter(control::MouseFilter::IGNORE);
        self.was_playing = self.game_world.bind().is_simulation_playing();
        self.refresh_label(if self.was_playing {
            SimulationPerformanceDisplay::Sampling
        } else {
            SimulationPerformanceDisplay::Paused
        });
        self.base_mut().set_process(true);
    }

    fn process(&mut self, delta: f64) {
        let (is_playing, sample) = {
            let game_world = self.game_world.bind();
            (
                game_world.is_simulation_playing(),
                game_world.tick_performance_sample(),
            )
        };

        if is_playing != self.was_playing {
            self.was_playing = is_playing;
            self.refresh_elapsed = 0.0;
            self.tick_accumulator.reset();
            self.refresh_label(if is_playing {
                SimulationPerformanceDisplay::Sampling
            } else {
                SimulationPerformanceDisplay::Paused
            });
        }

        if let Some(sample) = sample {
            if self.last_sample_sequence != Some(sample.sequence) {
                self.last_sample_sequence = Some(sample.sequence);
                if is_playing {
                    self.tick_accumulator.add(sample);
                }
            }
        }

        self.refresh_elapsed += delta;
        if self.refresh_elapsed < REFRESH_INTERVAL_SECONDS {
            return;
        }
        self.refresh_elapsed = 0.0;

        if is_playing {
            let display = self
                .tick_accumulator
                .take_report()
                .map_or(SimulationPerformanceDisplay::Sampling, |report| {
                    SimulationPerformanceDisplay::Report(report)
                });
            self.refresh_label(display);
        } else {
            self.tick_accumulator.reset();
            self.refresh_label(SimulationPerformanceDisplay::Paused);
        }
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if !event.is_action_pressed(ACTION_PERFORMANCE_INFO_TOGGLE) {
            return;
        }

        if self.base().is_visible() {
            self.base_mut().hide();
        } else {
            self.base_mut().show();
        }
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SimulationPerformanceDisplay {
    Sampling,
    Paused,
    Report(TickPerformanceReport),
}

impl PerformanceInfo {
    fn refresh_label(&mut self, display: SimulationPerformanceDisplay) {
        let fps = Engine::singleton().get_frames_per_second();
        self.info_label
            .clone()
            .set_text(performance_text(fps, display).as_str());
    }
}

fn performance_text(fps: f64, display: SimulationPerformanceDisplay) -> String {
    match display {
        SimulationPerformanceDisplay::Sampling => {
            format!("FPS: {fps:.0}\nWorld tick: Sampling...\nFixed tick: Sampling...")
        }
        SimulationPerformanceDisplay::Paused => {
            format!("FPS: {fps:.0}\nWorld tick: Paused\nFixed tick: Paused")
        }
        SimulationPerformanceDisplay::Report(report) => format!(
            "FPS: {fps:.0}\nWorld tick: {:.3} ms\nFixed tick: {:.3} ms",
            report.world_tick_milliseconds, report.fixed_tick_milliseconds
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulator_averages_world_calls_and_weights_fixed_ticks() {
        let mut accumulator = TickPerformanceAccumulator::default();
        accumulator.add(TickPerformanceSample {
            sequence: 1,
            wall_time: Duration::from_millis(4),
            fixed_tick_count: 2,
        });
        accumulator.add(TickPerformanceSample {
            sequence: 2,
            wall_time: Duration::from_millis(12),
            fixed_tick_count: 4,
        });

        let report = accumulator.take_report().expect("report should exist");
        assert_eq!(report.world_tick_milliseconds, 8.0);
        assert!((report.fixed_tick_milliseconds - (16.0 / 6.0)).abs() < f64::EPSILON);
        assert!(accumulator.take_report().is_none());
    }

    #[test]
    fn accumulator_ignores_paused_samples() {
        let mut accumulator = TickPerformanceAccumulator::default();
        accumulator.add(TickPerformanceSample {
            sequence: 1,
            wall_time: Duration::from_nanos(50),
            fixed_tick_count: 0,
        });

        assert!(accumulator.take_report().is_none());
    }

    #[test]
    fn performance_text_formats_report_and_paused_state() {
        assert_eq!(
            performance_text(
                59.6,
                SimulationPerformanceDisplay::Report(TickPerformanceReport {
                    world_tick_milliseconds: 4.1234,
                    fixed_tick_milliseconds: 0.0414,
                })
            ),
            "FPS: 60\nWorld tick: 4.123 ms\nFixed tick: 0.041 ms"
        );
        assert_eq!(
            performance_text(30.2, SimulationPerformanceDisplay::Paused),
            "FPS: 30\nWorld tick: Paused\nFixed tick: Paused"
        );
    }
}
