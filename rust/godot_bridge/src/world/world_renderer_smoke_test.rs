use crate::world::game_world::{GameWorld, RendererMode};
use godot::classes::{INode, Node};
use godot::obj::OnEditor;
use godot::prelude::*;
use std::time::{Duration, Instant};

const MAX_SMOKE_FRAMES: u32 = 900;
// Software rendering and movie capture can make a single 1440p frame take
// close to a second even though the normal headless smoke completes in a few
// seconds. Keep a wall-clock guard without making that diagnostic path flaky.
const MAX_SMOKE_DURATION: Duration = Duration::from_secs(120);
const SUCCESS_MARKER: &str = "WORLD_RENDERER_SMOKE_SUCCESS";
const FAILURE_MARKER: &str = "WORLD_RENDERER_SMOKE_FAILURE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmokeAction {
    SetSpeed(i32),
    SwitchRenderer(RendererMode),
    SwitchSurfaceIfAvailable,
}

const SMOKE_ACTIONS: [SmokeAction; 14] = [
    SmokeAction::SetSpeed(1),
    SmokeAction::SwitchRenderer(RendererMode::ThreeD),
    SmokeAction::SwitchRenderer(RendererMode::TwoD),
    SmokeAction::SwitchRenderer(RendererMode::ThreeD),
    SmokeAction::SwitchRenderer(RendererMode::TwoD),
    SmokeAction::SetSpeed(100),
    SmokeAction::SwitchRenderer(RendererMode::ThreeD),
    SmokeAction::SwitchRenderer(RendererMode::TwoD),
    SmokeAction::SwitchRenderer(RendererMode::ThreeD),
    SmokeAction::SwitchRenderer(RendererMode::TwoD),
    SmokeAction::SwitchSurfaceIfAvailable,
    SmokeAction::SwitchRenderer(RendererMode::ThreeD),
    SmokeAction::SwitchRenderer(RendererMode::TwoD),
    SmokeAction::SetSpeed(1),
];

/// End-to-end renderer transition test intended to run as a standalone,
/// headless Godot scene. All interaction with the game uses typed Rust calls;
/// the exported reference is the only scene-tree boundary.
#[derive(GodotClass)]
#[class(base = Node)]
pub(crate) struct WorldRendererSmokeTest {
    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    action_index: usize,
    frames_elapsed: u32,
    started_at: Instant,
    expected_mode: Option<RendererMode>,
    expected_speed: Option<i32>,
    expected_surface: Option<i32>,
    capture_path: Option<String>,
    capture_completed: bool,
    finished: bool,
    base: Base<Node>,
}

#[godot_api]
impl INode for WorldRendererSmokeTest {
    fn init(base: Base<Node>) -> Self {
        Self {
            game_world: OnEditor::default(),
            action_index: 0,
            frames_elapsed: 0,
            started_at: Instant::now(),
            expected_mode: None,
            expected_speed: None,
            expected_surface: None,
            capture_path: None,
            capture_completed: false,
            finished: false,
            base,
        }
    }

    fn ready(&mut self) {
        self.started_at = Instant::now();
        self.capture_path = std::env::var("BOAPSPACE_RENDERER_CAPTURE").ok();
        let mut game_world = self.game_world.clone();
        let mut game_world = game_world.bind_mut();
        if game_world.active_renderer_mode() != RendererMode::TwoD {
            drop(game_world);
            self.fail("game did not start in the 2D renderer");
            return;
        }
        if !game_world.is_simulation_playing() {
            game_world.toggle_simulation_playing();
        }
        drop(game_world);
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if self.finished {
            return;
        }
        self.frames_elapsed = self.frames_elapsed.saturating_add(1);
        if self.frames_elapsed > MAX_SMOKE_FRAMES || self.started_at.elapsed() > MAX_SMOKE_DURATION
        {
            self.fail("timed out waiting for renderer transitions to complete");
            return;
        }

        let mut game_world = self.game_world.clone();
        let mut game_world = game_world.bind_mut();
        if let Err(reason) = validate_expected_state(
            &game_world,
            self.expected_mode,
            self.expected_speed,
            self.expected_surface,
        ) {
            drop(game_world);
            self.fail(reason.as_str());
            return;
        }
        if self.expected_mode == Some(RendererMode::ThreeD)
            && !self.capture_completed
            && self.capture_path.is_some()
        {
            if let Err(reason) = self.capture_renderer_frame() {
                drop(game_world);
                self.fail(reason.as_str());
                return;
            }
        }
        self.expected_mode = None;
        self.expected_speed = None;
        self.expected_surface = None;

        if !game_world.renderer_mode_available(RendererMode::ThreeD) {
            let reason = game_world.renderer_mode_unavailable_reason(RendererMode::ThreeD);
            if reason != Some("Preparing 3D renderer assets.") {
                let reason = reason.unwrap_or("3D renderer became unavailable without a reason");
                drop(game_world);
                self.fail(reason);
            }
            return;
        }

        let Some(action) = SMOKE_ACTIONS.get(self.action_index).copied() else {
            drop(game_world);
            self.succeed();
            return;
        };

        let result = match action {
            SmokeAction::SetSpeed(multiplier) => {
                if !game_world.set_simulation_speed_multiplier(multiplier) {
                    Err(format!(
                        "GameWorld rejected supported simulation speed {multiplier}x"
                    ))
                } else if game_world.simulation_speed_multiplier() != multiplier {
                    Err(format!(
                        "simulation speed did not become {multiplier}x immediately"
                    ))
                } else {
                    self.expected_speed = Some(multiplier);
                    Ok(())
                }
            }
            SmokeAction::SwitchRenderer(mode) => {
                if !game_world.set_renderer_mode(mode) {
                    Err(format!(
                        "GameWorld rejected renderer transition to {mode:?}"
                    ))
                } else if game_world.active_renderer_mode() != mode {
                    Err(format!("renderer did not become {mode:?} immediately"))
                } else {
                    self.expected_mode = Some(mode);
                    Ok(())
                }
            }
            SmokeAction::SwitchSurfaceIfAvailable => {
                let count = game_world.surface_count();
                if count <= 1 {
                    godot_print!(
                        "WorldRendererSmokeTest: one surface available; surface switch skipped"
                    );
                    Ok(())
                } else {
                    let current = game_world.active_surface_index();
                    let target = (current + 1).rem_euclid(count);
                    if !game_world.set_active_surface_index(target) {
                        Err(format!("GameWorld rejected surface transition to {target}"))
                    } else if game_world.active_surface_index() != target {
                        Err(format!(
                            "active surface did not become {target} immediately"
                        ))
                    } else {
                        self.expected_surface = Some(target);
                        Ok(())
                    }
                }
            }
        };

        match result {
            Ok(()) => {
                self.action_index += 1;
                godot_print!(
                    "WorldRendererSmokeTest: completed action {}/{}: {:?}",
                    self.action_index,
                    SMOKE_ACTIONS.len(),
                    action
                );
            }
            Err(reason) => {
                drop(game_world);
                self.fail(reason.as_str());
            }
        }
    }
}

impl WorldRendererSmokeTest {
    fn capture_renderer_frame(&mut self) -> Result<(), String> {
        let path = self.capture_path.as_deref().unwrap_or_default();
        let viewport = self
            .base()
            .get_viewport()
            .ok_or_else(|| "renderer capture has no viewport".to_owned())?;
        let texture = viewport
            .get_texture()
            .ok_or_else(|| "renderer capture has no viewport texture".to_owned())?;
        let image = texture
            .get_image()
            .ok_or_else(|| "renderer capture could not read viewport pixels".to_owned())?;
        let error = image.save_png(path);
        if error != godot::global::Error::OK {
            return Err(format!("renderer capture could not save {path}: {error:?}"));
        }
        self.capture_completed = true;
        godot_print!("WorldRendererSmokeTest: captured active 3D frame to {path}");
        Ok(())
    }

    fn succeed(&mut self) {
        self.finished = true;
        godot_print!("{SUCCESS_MARKER}: repeated 2D/3D transitions passed at 1x and 100x");
        self.quit(0);
    }

    fn fail(&mut self, reason: &str) {
        self.finished = true;
        godot_error!("{FAILURE_MARKER}: {reason}");
        self.quit(1);
    }

    fn quit(&self, exit_code: i32) {
        let mut tree = self.base().get_tree();
        tree.quit_ex().exit_code(exit_code).done();
    }
}

fn validate_expected_state(
    game_world: &GameWorld,
    expected_mode: Option<RendererMode>,
    expected_speed: Option<i32>,
    expected_surface: Option<i32>,
) -> Result<(), String> {
    if let Some(expected) = expected_mode {
        let actual = game_world.active_renderer_mode();
        if actual != expected {
            return Err(format!(
                "renderer transition was not stable across a frame: expected {expected:?}, got {actual:?}"
            ));
        }
    }
    if let Some(expected) = expected_speed {
        let actual = game_world.simulation_speed_multiplier();
        if actual != expected {
            return Err(format!(
                "simulation speed was not stable across a frame: expected {expected}x, got {actual}x"
            ));
        }
    }
    if let Some(expected) = expected_surface {
        let actual = game_world.active_surface_index();
        if actual != expected {
            return Err(format!(
                "surface transition was not stable across a frame: expected {expected}, got {actual}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_sequence_repeats_both_directions_at_one_and_one_hundred_x() {
        let mut speed = 1;
        let mut switches_at_one = Vec::new();
        let mut switches_at_one_hundred = Vec::new();
        for action in SMOKE_ACTIONS {
            match action {
                SmokeAction::SetSpeed(multiplier) => speed = multiplier,
                SmokeAction::SwitchRenderer(mode) if speed == 1 => switches_at_one.push(mode),
                SmokeAction::SwitchRenderer(mode) if speed == 100 => {
                    switches_at_one_hundred.push(mode)
                }
                SmokeAction::SwitchRenderer(_) | SmokeAction::SwitchSurfaceIfAvailable => {}
            }
        }

        let repeated_cycle = [
            RendererMode::ThreeD,
            RendererMode::TwoD,
            RendererMode::ThreeD,
            RendererMode::TwoD,
        ];
        assert!(switches_at_one.starts_with(&repeated_cycle));
        assert!(switches_at_one_hundred.starts_with(&repeated_cycle));
        assert!(SMOKE_ACTIONS.contains(&SmokeAction::SwitchSurfaceIfAvailable));
    }
}
