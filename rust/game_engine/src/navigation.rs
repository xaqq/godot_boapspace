use std::collections::VecDeque;
use std::sync::Arc;

use bevy_ecs::prelude::*;

use crate::buildings::{Building, BuildingBlueprint, BuildingFootprint};
use crate::collision::building_blocks_npc_walk;
use crate::components::{
    MovementTarget, Npc, NpcPosition, ResourceNode, Terrain, TerrainKind, Tile, TilePosition,
};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::tile::TileIndex;

/// Immutable, surface-local view of the cells NPCs may currently enter.
///
/// Capturing collision once keeps path searches deterministic and avoids
/// repeatedly scanning ECS collision components for every expanded BFS cell.
#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct NavigationSnapshot {
    size: GridSize,
    walkable: Arc<Vec<bool>>,
    fingerprint: u64,
}

pub fn refresh_navigation_snapshot(world: &mut World) {
    if world.get_resource::<NavigationSnapshot>().is_some() {
        return;
    }
    if let Some(snapshot) = NavigationSnapshot::from_world(world) {
        world.insert_resource(snapshot);
    }
}

pub fn invalidate_navigation_snapshot(world: &mut World) {
    world.remove_resource::<NavigationSnapshot>();
}

pub fn current_navigation_snapshot(world: &World) -> Option<NavigationSnapshot> {
    world
        .get_resource::<NavigationSnapshot>()
        .cloned()
        .or_else(|| NavigationSnapshot::from_world(world))
}

impl NavigationSnapshot {
    pub fn from_world(world: &World) -> Option<Self> {
        let size = world.get_resource::<Grid>()?.size();
        let cell_count = size.cell_count()?;
        let tile_index = world.get_resource::<TileIndex>()?;
        let mut walkable = vec![false; cell_count];

        for coord in size.iter_coords() {
            let can_enter = tile_index
                .get(coord)
                .filter(|entity| world.get::<Tile>(*entity).is_some())
                .and_then(|entity| world.get::<Terrain>(entity))
                .is_some_and(|terrain| terrain.kind != TerrainKind::Water);
            set_walkability(size, &mut walkable, coord, can_enter);
        }

        if let Some(mut resources) = world.try_query::<(&TilePosition, &ResourceNode)>() {
            for (position, _) in resources.iter(world) {
                set_walkability(size, &mut walkable, position.coord, false);
            }
        }
        if let Some(mut blueprints) = world.try_query::<&BuildingBlueprint>() {
            for blueprint in blueprints.iter(world) {
                if building_blocks_npc_walk(blueprint.kind) {
                    for coord in blueprint.footprint.iter_coords() {
                        set_walkability(size, &mut walkable, coord, false);
                    }
                }
            }
        }
        if let Some(mut buildings) = world.try_query::<&Building>() {
            for building in buildings.iter(world) {
                if building_blocks_npc_walk(building.kind) {
                    for coord in building.footprint.iter_coords() {
                        set_walkability(size, &mut walkable, coord, false);
                    }
                }
            }
        }

        let fingerprint = walkability_fingerprint(size, &walkable);
        Some(Self {
            size,
            walkable: Arc::new(walkable),
            fingerprint,
        })
    }

    pub const fn size(&self) -> GridSize {
        self.size
    }

    pub fn is_walkable(&self, coord: CellCoord) -> bool {
        self.index(coord)
            .and_then(|index| self.walkable.get(index))
            .copied()
            .unwrap_or(false)
    }

    pub const fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Returns a deterministic cardinal shortest path, including both `start`
    /// and `goal`. The starting cell may have become blocked underneath an NPC;
    /// all cells entered after it must be walkable.
    pub fn shortest_path(&self, start: CellCoord, goal: CellCoord) -> Option<Vec<CellCoord>> {
        self.shortest_path_to_any(start, [goal])
            .map(|path| path.cells)
    }

    /// Selects the reachable goal with the shortest cardinal distance. Equal
    /// distances use lower y and then lower x, independently of caller order.
    pub fn shortest_path_to_any(
        &self,
        start: CellCoord,
        goals: impl IntoIterator<Item = CellCoord>,
    ) -> Option<NavigationPath> {
        let start_index = self.index(start)?;
        let mut goals = goals
            .into_iter()
            .filter(|goal| *goal == start || self.is_walkable(*goal))
            .collect::<Vec<_>>();
        goals.sort_unstable_by_key(|coord| (coord.y(), coord.x()));
        goals.dedup();
        if goals.is_empty() {
            return None;
        }

        let cell_count = self.size.cell_count()?;
        let mut distance = vec![usize::MAX; cell_count];
        let mut previous = vec![usize::MAX; cell_count];
        let mut queue = VecDeque::from([start]);
        distance[start_index] = 0;

        while let Some(coord) = queue.pop_front() {
            let coord_index = self.index(coord)?;
            for neighbor in self.cardinal_neighbors(coord) {
                let neighbor_index = self.index(neighbor)?;
                if distance[neighbor_index] != usize::MAX || !self.is_walkable(neighbor) {
                    continue;
                }
                distance[neighbor_index] = distance[coord_index] + 1;
                previous[neighbor_index] = coord_index;
                queue.push_back(neighbor);
            }
        }

        let target = goals.into_iter().min_by_key(|goal| {
            let target_distance = self
                .index(*goal)
                .map(|index| distance[index])
                .unwrap_or(usize::MAX);
            (target_distance, goal.y(), goal.x())
        })?;
        let target_index = self.index(target)?;
        let target_distance = distance[target_index];
        if target_distance == usize::MAX {
            return None;
        }

        let mut reversed = Vec::with_capacity(target_distance + 1);
        let mut cursor = target_index;
        loop {
            reversed.push(self.coord(cursor)?);
            if cursor == start_index {
                break;
            }
            cursor = previous[cursor];
            if cursor == usize::MAX {
                return None;
            }
        }
        reversed.reverse();

        Some(NavigationPath {
            target,
            distance: target_distance,
            cells: reversed,
        })
    }

    /// Walkable cardinal neighbors in row-major order.
    pub fn point_interaction_cells(&self, point: CellCoord) -> Vec<CellCoord> {
        self.cardinal_neighbors(point)
            .into_iter()
            .filter(|coord| self.is_walkable(*coord))
            .collect()
    }

    /// Walkable cells immediately outside a rectangular blocking footprint.
    pub fn exterior_interaction_cells(&self, footprint: BuildingFootprint) -> Vec<CellCoord> {
        let mut cells = Vec::new();
        for footprint_cell in footprint.iter_coords() {
            for candidate in cardinal_coords(footprint_cell) {
                if !footprint.contains(candidate)
                    && self.size.contains(candidate)
                    && self.is_walkable(candidate)
                {
                    cells.push(candidate);
                }
            }
        }
        sort_and_deduplicate(&mut cells);
        cells
    }

    /// Walkable footprint cells for targets such as Fields and Tree Plots.
    pub fn footprint_interaction_cells(&self, footprint: BuildingFootprint) -> Vec<CellCoord> {
        let mut cells = footprint
            .iter_coords()
            .filter(|coord| self.is_walkable(*coord))
            .collect::<Vec<_>>();
        sort_and_deduplicate(&mut cells);
        cells
    }

    fn cardinal_neighbors(&self, coord: CellCoord) -> Vec<CellCoord> {
        let mut neighbors = cardinal_coords(coord)
            .into_iter()
            .filter(|candidate| self.size.contains(*candidate))
            .collect::<Vec<_>>();
        neighbors.sort_unstable_by_key(|candidate| (candidate.y(), candidate.x()));
        neighbors
    }

    fn index(&self, coord: CellCoord) -> Option<usize> {
        if !self.size.contains(coord) {
            return None;
        }
        let x = usize::try_from(coord.x()).ok()?;
        let y = usize::try_from(coord.y()).ok()?;
        y.checked_mul(self.size.width())?.checked_add(x)
    }

    fn coord(&self, index: usize) -> Option<CellCoord> {
        if self.size.width() == 0 || index >= self.size.cell_count()? {
            return None;
        }
        CellCoord::from_usize(index % self.size.width(), index / self.size.width())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationPath {
    target: CellCoord,
    distance: usize,
    cells: Vec<CellCoord>,
}

impl NavigationPath {
    pub const fn target(&self) -> CellCoord {
        self.target
    }

    pub const fn distance(&self) -> usize {
        self.distance
    }

    pub fn cells(&self) -> &[CellCoord] {
        &self.cells
    }

    pub fn into_cells(self) -> Vec<CellCoord> {
        self.cells
    }
}

/// A persistent NPC navigation request. The route driver owns its queued
/// cardinal waypoints and replans them when surface collision changes.
#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct NpcRoute {
    goals: Vec<CellCoord>,
    waypoints: VecDeque<CellCoord>,
    destination: Option<CellCoord>,
    planned_fingerprint: Option<u64>,
}

impl NpcRoute {
    pub fn new(goals: impl IntoIterator<Item = CellCoord>) -> Self {
        let mut goals = goals.into_iter().collect::<Vec<_>>();
        sort_and_deduplicate(&mut goals);
        Self {
            goals,
            waypoints: VecDeque::new(),
            destination: None,
            planned_fingerprint: None,
        }
    }

    pub fn to_cell(goal: CellCoord) -> Self {
        Self::new([goal])
    }

    pub fn goals(&self) -> &[CellCoord] {
        &self.goals
    }

    pub const fn destination(&self) -> Option<CellCoord> {
        self.destination
    }

    pub fn waypoints(&self) -> impl Iterator<Item = CellCoord> + '_ {
        self.waypoints.iter().copied()
    }

    pub fn is_blocked(&self) -> bool {
        self.planned_fingerprint.is_some() && self.destination.is_none()
    }

    fn plan(&mut self, snapshot: &NavigationSnapshot, start: CellCoord) {
        self.waypoints.clear();
        self.destination = None;
        self.planned_fingerprint = Some(snapshot.fingerprint());

        let Some(path) = snapshot.shortest_path_to_any(start, self.goals.iter().copied()) else {
            return;
        };
        self.destination = Some(path.target());
        self.waypoints = path.into_cells().into_iter().skip(1).collect();
    }
}

/// Feeds the next queued cardinal waypoint into the existing sub-tile movement
/// system. NPCs without `NpcRoute` retain the legacy direct-target behavior.
pub fn drive_npc_routes(world: &mut World) {
    let Some(snapshot) = current_navigation_snapshot(world) else {
        return;
    };

    let mut query = world
        .query_filtered::<(Entity, &NpcPosition, &NpcRoute, Option<&MovementTarget>), With<Npc>>();
    let mut routed_npcs = query
        .iter(world)
        .map(|(entity, position, route, movement)| {
            (entity, *position, route.clone(), movement.copied())
        })
        .collect::<Vec<_>>();
    routed_npcs.sort_unstable_by_key(|(entity, ..)| entity.to_bits());

    for (entity, position, mut route, movement) in routed_npcs {
        let must_replan = route.planned_fingerprint != Some(snapshot.fingerprint());
        if must_replan {
            route.plan(&snapshot, position.coord);
        }

        if movement.is_none() {
            while route
                .waypoints
                .front()
                .is_some_and(|coord| *coord == position.coord)
            {
                route.waypoints.pop_front();
            }
        }

        if route.destination == Some(position.coord)
            && route.waypoints.is_empty()
            && movement.is_none()
        {
            world.entity_mut(entity).remove::<NpcRoute>();
            continue;
        }

        if route.destination.is_none() {
            world.entity_mut(entity).remove::<MovementTarget>();
            world.entity_mut(entity).insert(route);
            continue;
        }

        let next = route.waypoints.front().copied();
        let movement_matches = movement.map(|target| target.coord) == next;
        let mut entity_mut = world.entity_mut(entity);
        entity_mut.insert(route);
        match next {
            Some(next) if !movement_matches => {
                entity_mut.insert(MovementTarget::new(next));
            }
            None => {
                entity_mut.remove::<MovementTarget>();
            }
            Some(_) => {}
        }
    }
}

fn cardinal_coords(coord: CellCoord) -> Vec<CellCoord> {
    let mut cells = Vec::with_capacity(4);
    if let Some(y) = coord.y().checked_sub(1) {
        cells.push(CellCoord::new(coord.x(), y));
    }
    if let Some(x) = coord.x().checked_sub(1) {
        cells.push(CellCoord::new(x, coord.y()));
    }
    if let Some(x) = coord.x().checked_add(1) {
        cells.push(CellCoord::new(x, coord.y()));
    }
    if let Some(y) = coord.y().checked_add(1) {
        cells.push(CellCoord::new(coord.x(), y));
    }
    cells
}

fn sort_and_deduplicate(cells: &mut Vec<CellCoord>) {
    cells.sort_unstable_by_key(|coord| (coord.y(), coord.x()));
    cells.dedup();
}

fn set_walkability(size: GridSize, walkable: &mut [bool], coord: CellCoord, value: bool) {
    if !size.contains(coord) {
        return;
    }
    let Some(x) = usize::try_from(coord.x()).ok() else {
        return;
    };
    let Some(y) = usize::try_from(coord.y()).ok() else {
        return;
    };
    let Some(index) = y
        .checked_mul(size.width())
        .and_then(|row| row.checked_add(x))
    else {
        return;
    };
    if let Some(cell) = walkable.get_mut(index) {
        *cell = value;
    }
}

fn walkability_fingerprint(size: GridSize, walkable: &[bool]) -> u64 {
    // Stable FNV-1a; this is a change detector, not a persisted identifier.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in size
        .width()
        .to_le_bytes()
        .into_iter()
        .chain(size.height().to_le_bytes())
        .chain(walkable.iter().map(|value| u8::from(*value)))
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortest_path_can_exit_a_starting_cell_that_became_blocked() {
        let size = GridSize::new(3, 1);
        let walkable = vec![false, true, true];
        let snapshot = NavigationSnapshot {
            size,
            fingerprint: walkability_fingerprint(size, &walkable),
            walkable: Arc::new(walkable),
        };

        assert_eq!(
            snapshot.shortest_path(CellCoord::new(0, 0), CellCoord::new(2, 0)),
            Some(vec![
                CellCoord::new(0, 0),
                CellCoord::new(1, 0),
                CellCoord::new(2, 0),
            ])
        );
    }

    #[test]
    fn route_goals_are_normalized_to_row_major_unique_order() {
        let route = NpcRoute::new([
            CellCoord::new(2, 2),
            CellCoord::new(1, 1),
            CellCoord::new(2, 2),
            CellCoord::new(0, 1),
        ]);

        assert_eq!(
            route.goals(),
            &[
                CellCoord::new(0, 1),
                CellCoord::new(1, 1),
                CellCoord::new(2, 2),
            ]
        );
    }
}
