use super::task_table_view::{query_task_table_rows, TaskTableRow};
use crate::world::game_world::GameWorld;
use godot::classes::{control, Button, GridContainer, IPanelContainer, Label, PanelContainer};
use godot::obj::{NewAlloc, OnEditor};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct TaskListPanel {
    #[export]
    close_button: OnEditor<Gd<Button>>,

    #[export]
    row_container: OnEditor<Gd<GridContainer>>,

    #[export]
    toggle_button: OnEditor<Gd<Button>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    row_labels: Vec<Gd<Label>>,
    cached_rows: Option<Vec<TaskTableRow>>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for TaskListPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            close_button: OnEditor::default(),
            row_container: OnEditor::default(),
            toggle_button: OnEditor::default(),
            game_world: OnEditor::default(),
            row_labels: Vec::new(),
            cached_rows: None,
            base,
        }
    }

    fn ready(&mut self) {
        let close_button = self.close_button.clone();
        close_button
            .signals()
            .pressed()
            .connect_other(self, Self::hide_panel);

        let toggle_button = self.toggle_button.clone();
        toggle_button
            .signals()
            .pressed()
            .connect_other(self, Self::toggle_panel);

        self.base_mut().hide();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible() {
            self.refresh_rows();
        }
    }
}

impl TaskListPanel {
    fn hide_panel(&mut self) {
        self.base_mut().hide();
    }

    fn toggle_panel(&mut self) {
        if self.base().is_visible() {
            self.base_mut().hide();
        } else {
            self.base_mut().show();
            self.refresh_rows();
        }
    }

    fn refresh_rows(&mut self) {
        let rows = self
            .game_world
            .bind()
            .with_rendered_surface_world(query_task_table_rows);
        if self.cached_rows.as_ref() == Some(&rows) {
            return;
        }

        self.rebuild_rows(rows);
    }

    fn rebuild_rows(&mut self, rows: Vec<TaskTableRow>) {
        for mut label in self.row_labels.drain(..) {
            label.queue_free();
        }

        let mut row_container = self.row_container.clone();
        self.add_row(
            &mut row_container,
            [
                "Entity",
                "Type",
                "Assignment",
                "Worker",
                "Building",
                "Recipe",
                "Progress",
            ],
        );
        if rows.is_empty() {
            self.add_row(&mut row_container, ["", "No tasks", "", "", "", "", ""]);
        } else {
            for row in &rows {
                self.add_row(
                    &mut row_container,
                    [
                        row.entity_id.signal_value().to_string().as_str(),
                        row.task_type.as_str(),
                        row.assignment.as_str(),
                        row.worker.as_str(),
                        row.building.as_str(),
                        row.recipe.as_str(),
                        row.progress.as_str(),
                    ],
                );
            }
        }

        self.cached_rows = Some(rows);
    }

    fn add_row(&mut self, row_container: &mut Gd<GridContainer>, cells: [&str; 7]) {
        for text in cells {
            let mut label = Label::new_alloc();
            label.set_text(text);
            label.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
            row_container.add_child(&label);
            self.row_labels.push(label);
        }
    }
}
