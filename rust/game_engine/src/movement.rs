use crate::components::{
    MaxVelocity, MovementFacing, MovementTarget, NpcPosition, SubtileOffset, Velocity,
    HALF_SUBTILE_UNITS_PER_TILE, SUBTILE_UNITS_PER_TILE,
};
use crate::grid::{CellCoord, Grid};
use crate::roads::{Road, RoadMap};
use bevy_ecs::prelude::*;

pub fn update_npc_movement(
    mut commands: Commands,
    grid: Res<Grid>,
    roads: Option<Res<RoadMap>>,
    road_query: Query<&Road>,
    mut npcs: Query<(
        Entity,
        &mut NpcPosition,
        &mut Velocity,
        &MaxVelocity,
        &MovementTarget,
        Option<&mut MovementFacing>,
    )>,
) {
    for (entity, mut position, mut velocity, max_velocity, target, facing) in &mut npcs {
        if !grid.size().contains(target.coord) {
            *velocity = Velocity::ZERO;
            commands.entity(entity).remove::<MovementTarget>();
            continue;
        }

        let delta = target_delta_units(*position, target.coord);
        let (numerator, denominator) = roads
            .as_ref()
            .and_then(|roads| roads.entity_at(target.coord))
            .and_then(|entity| road_query.get(entity).ok())
            .map_or((1, 1), |road| road.tier.movement_ratio());
        let max_step = i64::from(max_velocity.units_per_tick).saturating_mul(i64::from(numerator))
            / i64::from(denominator);
        if delta == (0, 0) {
            *velocity = Velocity::ZERO;
            position.subtile_offset = SubtileOffset::ZERO;
            commands.entity(entity).remove::<MovementTarget>();
            continue;
        }

        if max_step == 0 {
            *velocity = Velocity::ZERO;
            continue;
        }

        if squared_distance(delta) <= max_step * max_step {
            *velocity = Velocity::new(delta.0 as i32, delta.1 as i32);
            position.coord = target.coord;
            position.subtile_offset = SubtileOffset::ZERO;
            if let Some(mut facing) = facing {
                if let Some(next_facing) = MovementFacing::from_velocity(*velocity) {
                    *facing = next_facing;
                }
            }
            *velocity = Velocity::ZERO;
            commands.entity(entity).remove::<MovementTarget>();
            continue;
        }

        let step = movement_step(delta, max_step);
        *velocity = Velocity::new(step.0, step.1);
        apply_step(&mut position, step);

        if let Some(mut facing) = facing {
            if let Some(next_facing) = MovementFacing::from_velocity(*velocity) {
                *facing = next_facing;
            }
        }
    }
}

fn target_delta_units(position: NpcPosition, target: CellCoord) -> (i64, i64) {
    (
        (i64::from(target.x()) - i64::from(position.coord.x())) * i64::from(SUBTILE_UNITS_PER_TILE)
            - i64::from(position.subtile_offset.x_units),
        (i64::from(target.y()) - i64::from(position.coord.y())) * i64::from(SUBTILE_UNITS_PER_TILE)
            - i64::from(position.subtile_offset.y_units),
    )
}

fn squared_distance(delta: (i64, i64)) -> i64 {
    delta.0 * delta.0 + delta.1 * delta.1
}

fn movement_step(delta: (i64, i64), max_step: i64) -> (i32, i32) {
    debug_assert!(max_step > 0);

    let distance = (squared_distance(delta) as f64).sqrt();
    let x = floored_component(delta.0, distance, max_step);
    let y = floored_component(delta.1, distance, max_step);

    if x != 0 || y != 0 {
        return (x, y);
    }

    if delta.0.abs() >= delta.1.abs() {
        (delta.0.signum() as i32, 0)
    } else {
        (0, delta.1.signum() as i32)
    }
}

fn floored_component(value: i64, distance: f64, max_step: i64) -> i32 {
    let scaled = ((value.abs() as f64 / distance) * max_step as f64).floor() as i32;
    scaled * value.signum() as i32
}

fn apply_step(position: &mut NpcPosition, step: (i32, i32)) {
    let x_units = position.subtile_offset.x_units + step.0;
    let y_units = position.subtile_offset.y_units + step.1;
    let (x_coord_delta, x_offset) = normalize_axis(x_units);
    let (y_coord_delta, y_offset) = normalize_axis(y_units);

    position.coord = CellCoord::new(
        position.coord.x() + x_coord_delta,
        position.coord.y() + y_coord_delta,
    );
    position.subtile_offset = SubtileOffset::new(x_offset, y_offset);
}

fn normalize_axis(mut units: i32) -> (i32, i32) {
    let mut coord_delta = 0;
    while units >= HALF_SUBTILE_UNITS_PER_TILE {
        coord_delta += 1;
        units -= SUBTILE_UNITS_PER_TILE;
    }
    while units < -HALF_SUBTILE_UNITS_PER_TILE {
        coord_delta -= 1;
        units += SUBTILE_UNITS_PER_TILE;
    }

    (coord_delta, units)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Npc, DEFAULT_MAX_VELOCITY_UNITS_PER_TICK};
    use bevy_ecs::system::RunSystemOnce;

    #[test]
    fn test_cardinal_movement_advances_by_default_speed() {
        let mut world = movement_world(Grid::new(4, 4));
        let entity = spawn_moving_npc(&mut world, CellCoord::new(1, 1), CellCoord::new(2, 1));

        run_movement(&mut world);

        let position = *world
            .get::<NpcPosition>(entity)
            .expect("position should exist");
        assert_eq!(position.coord, CellCoord::new(1, 1));
        assert_eq!(
            position.subtile_offset,
            SubtileOffset::new(DEFAULT_MAX_VELOCITY_UNITS_PER_TICK as i32, 0)
        );
        assert_eq!(
            *world
                .get::<Velocity>(entity)
                .expect("velocity should exist"),
            Velocity::new(DEFAULT_MAX_VELOCITY_UNITS_PER_TICK as i32, 0)
        );
        assert_eq!(
            *world
                .get::<MovementFacing>(entity)
                .expect("facing should exist"),
            MovementFacing::East
        );
    }

    #[test]
    fn completed_destination_road_multiplies_speed_without_mutating_base_velocity() {
        for (tier, expected) in [
            (crate::roads::RoadTier::DirtPath, 24),
            (crate::roads::RoadTier::Cobblestone, 32),
            (crate::roads::RoadTier::Flagstone, 48),
        ] {
            let mut world = movement_world(Grid::new(4, 4));
            world.insert_resource(RoadMap::default());
            let coord = CellCoord::new(2, 1);
            let road = world.spawn(Road { coord, tier }).id();
            world.resource_mut::<RoadMap>().insert(coord, road);
            let entity = spawn_moving_npc(&mut world, CellCoord::new(1, 1), coord);

            run_movement(&mut world);

            assert_eq!(
                world.get::<Velocity>(entity).unwrap().x_units_per_tick,
                expected
            );
            assert_eq!(
                world.get::<MaxVelocity>(entity).unwrap().units_per_tick,
                DEFAULT_MAX_VELOCITY_UNITS_PER_TICK
            );
        }
    }

    #[test]
    fn test_diagonal_movement_is_not_faster_than_cardinal_movement() {
        let mut world = movement_world(Grid::new(4, 4));
        let entity = spawn_moving_npc(&mut world, CellCoord::new(1, 1), CellCoord::new(2, 2));

        run_movement(&mut world);

        let velocity = *world
            .get::<Velocity>(entity)
            .expect("velocity should exist");
        let speed_squared = i64::from(velocity.x_units_per_tick).pow(2)
            + i64::from(velocity.y_units_per_tick).pow(2);
        let max_speed_squared = i64::from(DEFAULT_MAX_VELOCITY_UNITS_PER_TICK).pow(2);
        assert!(speed_squared <= max_speed_squared);
        assert_eq!(velocity, Velocity::new(11, 11));
        assert_eq!(
            *world
                .get::<MovementFacing>(entity)
                .expect("facing should exist"),
            MovementFacing::SouthEast
        );
    }

    #[test]
    fn test_tile_boundary_crossing_updates_coord_and_normalizes_offset() {
        let mut world = movement_world(Grid::new(4, 4));
        let entity = world
            .spawn((
                Npc,
                NpcPosition {
                    coord: CellCoord::new(1, 1),
                    subtile_offset: SubtileOffset::new(500, 0),
                },
                Velocity::ZERO,
                MaxVelocity::default(),
                MovementTarget::new(CellCoord::new(2, 1)),
                MovementFacing::default(),
            ))
            .id();

        run_movement(&mut world);

        let position = *world
            .get::<NpcPosition>(entity)
            .expect("position should exist");
        assert_eq!(position.coord, CellCoord::new(2, 1));
        assert_eq!(position.subtile_offset, SubtileOffset::new(-508, 0));
    }

    #[test]
    fn test_arrival_snaps_to_target_and_removes_target() {
        let mut world = movement_world(Grid::new(4, 4));
        let entity = world
            .spawn((
                Npc,
                NpcPosition {
                    coord: CellCoord::new(1, 1),
                    subtile_offset: SubtileOffset::new(500, 0),
                },
                Velocity::ZERO,
                MaxVelocity::default(),
                MovementTarget::new(CellCoord::new(2, 1)),
                MovementFacing::default(),
            ))
            .id();

        for _ in 0..40 {
            run_movement(&mut world);
        }

        let position = *world
            .get::<NpcPosition>(entity)
            .expect("position should exist");
        assert_eq!(position.coord, CellCoord::new(2, 1));
        assert_eq!(position.subtile_offset, SubtileOffset::ZERO);
        assert_eq!(
            *world
                .get::<Velocity>(entity)
                .expect("velocity should exist"),
            Velocity::ZERO
        );
        assert!(world.get::<MovementTarget>(entity).is_none());
    }

    #[test]
    fn test_invalid_target_stops_movement_and_removes_target() {
        let mut world = movement_world(Grid::new(2, 2));
        let entity = spawn_moving_npc(&mut world, CellCoord::new(1, 1), CellCoord::new(3, 1));

        run_movement(&mut world);

        assert_eq!(
            *world
                .get::<Velocity>(entity)
                .expect("velocity should exist"),
            Velocity::ZERO
        );
        assert!(world.get::<MovementTarget>(entity).is_none());
    }

    fn movement_world(grid: Grid) -> World {
        let mut world = World::new();
        world.insert_resource(grid);
        world
    }

    fn spawn_moving_npc(world: &mut World, coord: CellCoord, target: CellCoord) -> Entity {
        world
            .spawn((
                Npc,
                NpcPosition::new(coord),
                Velocity::ZERO,
                MaxVelocity::default(),
                MovementTarget::new(target),
                MovementFacing::default(),
            ))
            .id()
    }

    fn run_movement(world: &mut World) {
        world
            .run_system_once(update_npc_movement)
            .expect("movement system should run");
    }
}
