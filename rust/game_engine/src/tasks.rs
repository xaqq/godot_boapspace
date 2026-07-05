use crate::buildings::BuildingBlueprint;
use bevy_ecs::prelude::*;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Task;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct ProgressBuildingConstruction {
    blueprint: Entity,
}

impl ProgressBuildingConstruction {
    pub const fn new(blueprint: Entity) -> Self {
        Self { blueprint }
    }

    pub const fn blueprint(self) -> Entity {
        self.blueprint
    }

    pub const fn label() -> &'static str {
        "ProgressBuildingConstruction"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Bundle)]
pub struct ProgressBuildingConstructionTaskBundle {
    task: Task,
    construction: ProgressBuildingConstruction,
}

impl ProgressBuildingConstructionTaskBundle {
    pub const fn new(blueprint: Entity) -> Self {
        Self {
            task: Task,
            construction: ProgressBuildingConstruction::new(blueprint),
        }
    }
}

pub fn maintain_construction_tasks(
    mut commands: Commands,
    blueprints: Query<Entity, With<BuildingBlueprint>>,
    tasks: Query<(Entity, &ProgressBuildingConstruction)>,
) {
    let blueprint_entities = blueprints.iter().collect::<HashSet<_>>();
    let mut represented_blueprints = HashSet::new();

    for (task_entity, task) in &tasks {
        let blueprint = task.blueprint();
        if !blueprint_entities.contains(&blueprint) || !represented_blueprints.insert(blueprint) {
            commands.entity(task_entity).despawn();
        }
    }

    for blueprint in blueprint_entities {
        if !represented_blueprints.contains(&blueprint) {
            commands.spawn(ProgressBuildingConstructionTaskBundle::new(blueprint));
        }
    }
}
