use crate::buildings::BuildingBlueprint;
use bevy_ecs::prelude::*;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Task {
    kind: TaskKind,
}

impl Task {
    pub const fn progress_building_construction(blueprint: Entity) -> Self {
        Self {
            kind: TaskKind::ProgressBuildingConstruction { blueprint },
        }
    }

    pub const fn kind(self) -> TaskKind {
        self.kind
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    ProgressBuildingConstruction { blueprint: Entity },
}

impl TaskKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ProgressBuildingConstruction { .. } => "ProgressBuildingConstruction",
        }
    }
}

pub fn maintain_construction_tasks(
    mut commands: Commands,
    blueprints: Query<Entity, With<BuildingBlueprint>>,
    tasks: Query<(Entity, &Task)>,
) {
    let blueprint_entities = blueprints.iter().collect::<HashSet<_>>();
    let mut represented_blueprints = HashSet::new();

    for (task_entity, task) in &tasks {
        match task.kind() {
            TaskKind::ProgressBuildingConstruction { blueprint } => {
                if !blueprint_entities.contains(&blueprint)
                    || !represented_blueprints.insert(blueprint)
                {
                    commands.entity(task_entity).despawn();
                }
            }
        }
    }

    for blueprint in blueprint_entities {
        if !represented_blueprints.contains(&blueprint) {
            commands.spawn(Task::progress_building_construction(blueprint));
        }
    }
}
