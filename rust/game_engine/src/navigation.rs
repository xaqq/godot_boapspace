use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::sync::Arc;

use bevy_ecs::prelude::*;

use crate::buildings::{Building, BuildingBlueprint, BuildingFootprint};
use crate::collision::{building_blocks_npc_walk, collision_flags_at};
use crate::components::{
    MovementTarget, Npc, NpcPosition, ResourceNode, Terrain, TerrainKind, Tile, TilePosition,
};
use crate::grid::{CellCoord, Grid, GridSize};
use crate::roads::{completed_road_tier_at, Road, NORMAL_TRAVERSAL_WEIGHT};
use crate::tile::TileIndex;

/// Immutable, surface-local view of the cells NPCs may currently enter.
///
/// Capturing collision once keeps path searches deterministic and avoids
/// repeatedly scanning ECS collision and road components for every expanded
/// weighted-search cell.
#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct NavigationSnapshot {
    size: GridSize,
    walkable: Arc<Vec<bool>>,
    traversal_weights: Arc<Vec<u32>>,
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

/// Re-evaluates walkability only for cells whose collision occupants changed.
/// The snapshot revision advances only when at least one cell actually changes.
pub(crate) fn refresh_navigation_snapshot_cells(
    world: &mut World,
    cells: impl IntoIterator<Item = CellCoord>,
) {
    if world.get_resource::<NavigationSnapshot>().is_none() {
        refresh_navigation_snapshot(world);
    }

    let mut cells = cells.into_iter().collect::<Vec<_>>();
    sort_and_deduplicate(&mut cells);
    let updates = cells
        .into_iter()
        .map(|coord| {
            let walkable =
                collision_flags_at(world, coord).is_some_and(|flags| !flags.is_npc_walk_blocked());
            let weight = completed_road_tier_at(world, coord)
                .map_or(NORMAL_TRAVERSAL_WEIGHT, |tier| tier.traversal_weight());
            (coord, walkable, weight)
        })
        .collect::<Vec<_>>();

    let Some(mut snapshot) = world.get_resource_mut::<NavigationSnapshot>() else {
        return;
    };
    let mut changed = false;
    for (coord, walkable, weight) in updates {
        changed |= snapshot.set_walkability(coord, walkable);
        changed |= snapshot.set_traversal_weight(coord, weight);
    }
    if changed {
        snapshot.fingerprint = snapshot.fingerprint.wrapping_add(1);
    }
}

/// Fully rebuilds the cached snapshot. Prefer refreshing affected cells when
/// their coordinates are known.
pub fn invalidate_navigation_snapshot(world: &mut World) {
    let replacement = NavigationSnapshot::from_world(world);
    match (world.get_resource::<NavigationSnapshot>(), replacement) {
        (Some(current), Some(mut replacement)) => {
            if current.size == replacement.size
                && current.walkable == replacement.walkable
                && current.traversal_weights == replacement.traversal_weights
            {
                return;
            }
            replacement.fingerprint = current.fingerprint.wrapping_add(1);
            world.insert_resource(replacement);
        }
        (None, Some(replacement)) => {
            world.insert_resource(replacement);
        }
        (_, None) => {
            world.remove_resource::<NavigationSnapshot>();
        }
    }
}

pub fn current_navigation_snapshot(world: &mut World) -> Option<NavigationSnapshot> {
    refresh_navigation_snapshot(world);
    world.get_resource::<NavigationSnapshot>().cloned()
}

impl NavigationSnapshot {
    pub fn from_world(world: &World) -> Option<Self> {
        let size = world.get_resource::<Grid>()?.size();
        let cell_count = size.cell_count()?;
        let tile_index = world.get_resource::<TileIndex>()?;
        let mut walkable = vec![false; cell_count];
        let mut traversal_weights = vec![NORMAL_TRAVERSAL_WEIGHT; cell_count];

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

        if let Some(mut roads) = world.try_query::<&Road>() {
            for road in roads.iter(world) {
                set_traversal_weight(
                    size,
                    &mut traversal_weights,
                    road.coord,
                    road.tier.traversal_weight(),
                );
            }
        }

        let fingerprint = navigation_fingerprint(size, &walkable, &traversal_weights);
        Some(Self {
            size,
            walkable: Arc::new(walkable),
            traversal_weights: Arc::new(traversal_weights),
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

    pub fn traversal_weight(&self, coord: CellCoord) -> Option<u32> {
        self.index(coord)
            .and_then(|index| self.traversal_weights.get(index))
            .copied()
    }

    /// Computes cardinal distances from `start` once for reuse across many
    /// target sets. A blocked starting cell remains a valid origin.
    pub(crate) fn distances_from(&self, start: CellCoord) -> Option<NavigationDistances> {
        let start_index = self.index(start)?;
        let cell_count = self.size.cell_count()?;
        if cell_count >= u32::MAX as usize {
            return None;
        }
        let mut distances = vec![u32::MAX; cell_count];
        let mut queue = BinaryHeap::from([Reverse((0_u32, 0_u64, start_index))]);
        let mut sequence = 1_u64;
        distances[start_index] = 0;

        while let Some(Reverse((distance, _, coord_index))) = queue.pop() {
            if distance != distances[coord_index] {
                continue;
            }
            let coord = self.coord(coord_index)?;
            for neighbor in cardinal_coords(coord).filter(|coord| self.size.contains(*coord)) {
                let neighbor_index = self.index(neighbor)?;
                if !self.is_walkable(neighbor) {
                    continue;
                }
                let next_distance = distance.checked_add(self.traversal_weight(neighbor)?)?;
                if next_distance >= distances[neighbor_index] {
                    continue;
                }
                distances[neighbor_index] = next_distance;
                queue.push(Reverse((next_distance, sequence, neighbor_index)));
                sequence = sequence.wrapping_add(1);
            }
        }

        Some(NavigationDistances {
            size: self.size,
            distances,
        })
    }

    /// Returns a deterministic cardinal shortest path, including both `start`
    /// and `goal`. The starting cell may have become blocked underneath an NPC;
    /// all cells entered after it must be walkable.
    pub fn shortest_path(&self, start: CellCoord, goal: CellCoord) -> Option<Vec<CellCoord>> {
        self.shortest_path_to_any(start, [goal])
            .map(|path| path.cells)
    }

    /// Selects the reachable goal with the lowest travel cost. Equal costs use
    /// lower y and then lower x, independently of caller order.
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
        if cell_count >= u32::MAX as usize {
            return None;
        }
        let mut previous = vec![u32::MAX; cell_count];
        previous[start_index] = u32::try_from(start_index).ok()?;
        let mut distances = vec![u32::MAX; cell_count];
        distances[start_index] = 0;
        let mut queue = BinaryHeap::from([Reverse((0_u32, 0_u64, start_index))]);
        let mut sequence = 1_u64;
        let mut goal_indices = goals
            .iter()
            .filter_map(|goal| self.index(*goal))
            .collect::<Vec<_>>();
        goal_indices.sort_unstable();
        let mut found_goals = Vec::new();
        let mut found_distance = None;
        if goal_indices.binary_search(&start_index).is_ok() {
            found_goals.push((start, 0_u32));
            found_distance = Some(0_u32);
        }

        while let Some(Reverse((coord_distance, _, coord_index))) = queue.pop() {
            if coord_distance != distances[coord_index] {
                continue;
            }
            if found_distance.is_some_and(|found| coord_distance > found) {
                break;
            }
            let coord = self.coord(coord_index)?;
            if goal_indices.binary_search(&coord_index).is_ok()
                && !found_goals.iter().any(|(goal, _)| *goal == coord)
            {
                found_distance = Some(coord_distance);
                found_goals.push((coord, coord_distance));
            }
            for neighbor in cardinal_coords(coord).filter(|coord| self.size.contains(*coord)) {
                let neighbor_index = self.index(neighbor)?;
                if !self.is_walkable(neighbor) {
                    continue;
                }
                let neighbor_distance =
                    coord_distance.checked_add(self.traversal_weight(neighbor)?)?;
                if neighbor_distance >= distances[neighbor_index] {
                    continue;
                }
                distances[neighbor_index] = neighbor_distance;
                previous[neighbor_index] = u32::try_from(coord_index).ok()?;
                queue.push(Reverse((neighbor_distance, sequence, neighbor_index)));
                sequence = sequence.wrapping_add(1);
            }
        }

        let (target, target_distance) = found_goals
            .into_iter()
            .min_by_key(|(goal, distance)| (*distance, goal.y(), goal.x()))?;
        let target_index = self.index(target)?;

        let mut reversed = Vec::new();
        let mut cursor = target_index;
        loop {
            reversed.push(self.coord(cursor)?);
            if cursor == start_index {
                break;
            }
            let predecessor = previous[cursor];
            if predecessor == u32::MAX {
                return None;
            }
            cursor = usize::try_from(predecessor).ok()?;
        }
        reversed.reverse();

        Some(NavigationPath {
            target,
            distance: usize::try_from(target_distance).ok()?,
            cells: reversed,
        })
    }

    /// Walkable cardinal neighbors in row-major order.
    pub fn point_interaction_cells(&self, point: CellCoord) -> Vec<CellCoord> {
        cardinal_coords(point)
            .filter(|coord| self.size.contains(*coord))
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

    fn set_walkability(&mut self, coord: CellCoord, value: bool) -> bool {
        let Some(index) = self.index(coord) else {
            return false;
        };
        if self.walkable.get(index).copied() == Some(value) {
            return false;
        }
        let walkable = Arc::make_mut(&mut self.walkable);
        let Some(cell) = walkable.get_mut(index) else {
            return false;
        };
        *cell = value;
        true
    }

    fn set_traversal_weight(&mut self, coord: CellCoord, value: u32) -> bool {
        let Some(index) = self.index(coord) else {
            return false;
        };
        if self.traversal_weights.get(index).copied() == Some(value) {
            return false;
        }
        let weights = Arc::make_mut(&mut self.traversal_weights);
        let Some(cell) = weights.get_mut(index) else {
            return false;
        };
        *cell = value;
        true
    }
}

/// Reusable weighted travel costs from one origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavigationDistances {
    size: GridSize,
    distances: Vec<u32>,
}

impl NavigationDistances {
    pub(crate) fn is_reachable(&self, goal: CellCoord) -> bool {
        navigation_index(self.size, goal)
            .and_then(|index| self.distances.get(index))
            .is_some_and(|distance| *distance != u32::MAX)
    }

    /// Returns the reachable goal ordered by travel cost, then row and column.
    pub(crate) fn closest_reachable(
        &self,
        goals: impl IntoIterator<Item = CellCoord>,
    ) -> Option<(CellCoord, usize)> {
        goals
            .into_iter()
            .filter_map(|goal| {
                let index = navigation_index(self.size, goal)?;
                let distance = *self.distances.get(index)?;
                if distance == u32::MAX {
                    return None;
                }
                Some((goal, usize::try_from(distance).ok()?))
            })
            .min_by_key(|(goal, distance)| (*distance, goal.y(), goal.x()))
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

    let mut query = world.query_filtered::<Entity, (With<Npc>, With<NpcRoute>)>();
    let mut routed_npcs = query.iter(world).collect::<Vec<_>>();
    routed_npcs.sort_unstable_by_key(|entity| entity.to_bits());

    let mut actions = Vec::with_capacity(routed_npcs.len());
    for entity in routed_npcs {
        let Some(position) = world.get::<NpcPosition>(entity).copied() else {
            continue;
        };
        let movement = world.get::<MovementTarget>(entity).copied();
        let Some(mut route) = world.get_mut::<NpcRoute>(entity) else {
            continue;
        };

        if route.planned_fingerprint != Some(snapshot.fingerprint()) {
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

        let action = if route.destination == Some(position.coord)
            && route.waypoints.is_empty()
            && movement.is_none()
        {
            RouteAction::RemoveRoute
        } else if route.destination.is_none() {
            RouteAction::RemoveMovement
        } else {
            let next = route.waypoints.front().copied();
            match next {
                Some(next) if movement.map(|target| target.coord) != Some(next) => {
                    RouteAction::SetMovement(next)
                }
                None => RouteAction::RemoveMovement,
                Some(_) => RouteAction::None,
            }
        };
        actions.push((entity, action));
    }

    for (entity, action) in actions {
        let mut entity_mut = world.entity_mut(entity);
        match action {
            RouteAction::RemoveRoute => {
                entity_mut.remove::<NpcRoute>();
            }
            RouteAction::RemoveMovement => {
                entity_mut.remove::<MovementTarget>();
            }
            RouteAction::SetMovement(coord) => {
                entity_mut.insert(MovementTarget::new(coord));
            }
            RouteAction::None => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteAction {
    RemoveRoute,
    RemoveMovement,
    SetMovement(CellCoord),
    None,
}

fn cardinal_coords(coord: CellCoord) -> impl Iterator<Item = CellCoord> {
    [
        coord
            .y()
            .checked_sub(1)
            .map(|y| CellCoord::new(coord.x(), y)),
        coord
            .x()
            .checked_sub(1)
            .map(|x| CellCoord::new(x, coord.y())),
        coord
            .x()
            .checked_add(1)
            .map(|x| CellCoord::new(x, coord.y())),
        coord
            .y()
            .checked_add(1)
            .map(|y| CellCoord::new(coord.x(), y)),
    ]
    .into_iter()
    .flatten()
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

fn navigation_index(size: GridSize, coord: CellCoord) -> Option<usize> {
    if !size.contains(coord) {
        return None;
    }
    let x = usize::try_from(coord.x()).ok()?;
    let y = usize::try_from(coord.y()).ok()?;
    y.checked_mul(size.width())?.checked_add(x)
}

fn set_traversal_weight(size: GridSize, weights: &mut [u32], coord: CellCoord, value: u32) {
    if let Some(index) = navigation_index(size, coord) {
        if let Some(weight) = weights.get_mut(index) {
            *weight = value;
        }
    }
}

fn navigation_fingerprint(size: GridSize, walkable: &[bool], weights: &[u32]) -> u64 {
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
    for weight in weights {
        for byte in weight.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingBlueprintBundle, BuildingKind};
    use crate::resources::ResourceKind;
    use crate::tile::{TileBundle, TileIndex};

    #[test]
    fn shortest_path_can_exit_a_starting_cell_that_became_blocked() {
        let size = GridSize::new(3, 1);
        let walkable = vec![false, true, true];
        let weights = vec![NORMAL_TRAVERSAL_WEIGHT; 3];
        let snapshot = NavigationSnapshot {
            size,
            fingerprint: navigation_fingerprint(size, &walkable, &weights),
            walkable: Arc::new(walkable),
            traversal_weights: Arc::new(weights),
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

    #[test]
    fn reusable_distances_choose_reachable_row_major_ties() {
        let size = GridSize::new(5, 3);
        let mut walkable = vec![true; size.cell_count().expect("small test grid")];
        for y in 0..3 {
            set_walkability(size, &mut walkable, CellCoord::new(3, y), false);
        }
        let snapshot = NavigationSnapshot {
            size,
            fingerprint: navigation_fingerprint(
                size,
                &walkable,
                &vec![NORMAL_TRAVERSAL_WEIGHT; size.cell_count().unwrap()],
            ),
            walkable: Arc::new(walkable),
            traversal_weights: Arc::new(vec![NORMAL_TRAVERSAL_WEIGHT; size.cell_count().unwrap()]),
        };

        let distances = snapshot
            .distances_from(CellCoord::new(1, 1))
            .expect("origin should be in bounds");

        assert_eq!(
            distances.closest_reachable([
                CellCoord::new(4, 1), // isolated by the wall
                CellCoord::new(2, 2),
                CellCoord::new(0, 0),
                CellCoord::new(2, 2),
            ]),
            Some((CellCoord::new(0, 0), 12))
        );
        assert_eq!(distances.closest_reachable([CellCoord::new(4, 1)]), None);
    }

    #[test]
    fn reusable_distances_can_exit_a_blocked_origin() {
        let size = GridSize::new(3, 1);
        let walkable = vec![false, true, true];
        let weights = vec![NORMAL_TRAVERSAL_WEIGHT; 3];
        let snapshot = NavigationSnapshot {
            size,
            fingerprint: navigation_fingerprint(size, &walkable, &weights),
            walkable: Arc::new(walkable),
            traversal_weights: Arc::new(weights),
        };
        let distances = snapshot
            .distances_from(CellCoord::new(0, 0))
            .expect("origin should be in bounds");

        assert_eq!(
            distances.closest_reachable([CellCoord::new(2, 0), CellCoord::new(0, 0)]),
            Some((CellCoord::new(0, 0), 0))
        );
        assert_eq!(
            distances.closest_reachable([CellCoord::new(2, 0)]),
            Some((CellCoord::new(2, 0), 12))
        );
    }

    #[test]
    fn targeted_refresh_changes_revision_only_when_walkability_changes() {
        let mut world = navigation_world(3, 2);
        let coord = CellCoord::new(1, 0);
        let initial = current_navigation_snapshot(&mut world).expect("snapshot should build");

        refresh_navigation_snapshot_cells(&mut world, [coord, coord]);
        let unchanged = current_navigation_snapshot(&mut world).expect("snapshot should remain");
        assert_eq!(unchanged.fingerprint(), initial.fingerprint());

        let tile = world
            .resource::<TileIndex>()
            .get(coord)
            .expect("tile should be indexed");
        world.entity_mut(tile).insert(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 1,
        });
        refresh_navigation_snapshot_cells(&mut world, [coord]);
        let blocked = current_navigation_snapshot(&mut world).expect("snapshot should remain");
        assert!(!blocked.is_walkable(coord));
        assert_ne!(blocked.fingerprint(), initial.fingerprint());

        let blocked_revision = blocked.fingerprint();
        refresh_navigation_snapshot_cells(&mut world, [coord]);
        assert_eq!(
            current_navigation_snapshot(&mut world)
                .expect("snapshot should remain")
                .fingerprint(),
            blocked_revision
        );

        world.entity_mut(tile).remove::<ResourceNode>();
        refresh_navigation_snapshot_cells(&mut world, [coord]);
        let reopened = current_navigation_snapshot(&mut world).expect("snapshot should remain");
        assert!(reopened.is_walkable(coord));
        assert_ne!(reopened.fingerprint(), blocked_revision);
    }

    #[test]
    fn non_blocking_blueprint_does_not_advance_navigation_revision() {
        let mut world = navigation_world(3, 2);
        let initial = current_navigation_snapshot(&mut world).expect("snapshot should build");
        let footprint = BuildingFootprint::new(CellCoord::new(0, 0), 2, 1);
        world.spawn(BuildingBlueprintBundle::new(BuildingKind::Field, footprint));

        refresh_navigation_snapshot_cells(&mut world, footprint.iter_coords());
        let refreshed = current_navigation_snapshot(&mut world).expect("snapshot should remain");

        assert_eq!(refreshed.fingerprint(), initial.fingerprint());
        assert!(footprint
            .iter_coords()
            .all(|coord| refreshed.is_walkable(coord)));
    }

    #[test]
    fn blocking_blueprint_refreshes_its_footprint_and_advances_revision() {
        let mut world = navigation_world(4, 2);
        let initial = current_navigation_snapshot(&mut world).expect("snapshot should build");
        let footprint = BuildingFootprint::new(CellCoord::new(1, 0), 2, 1);
        world.spawn(BuildingBlueprintBundle::new(
            BuildingKind::Warehouse,
            footprint,
        ));

        refresh_navigation_snapshot_cells(&mut world, footprint.iter_coords());
        let refreshed = current_navigation_snapshot(&mut world).expect("snapshot should remain");

        assert_ne!(refreshed.fingerprint(), initial.fingerprint());
        assert!(footprint
            .iter_coords()
            .all(|coord| !refreshed.is_walkable(coord)));
    }

    fn navigation_world(width: usize, height: usize) -> World {
        let size = GridSize::new(width, height);
        let mut world = World::new();
        world.insert_resource(Grid::new(width, height));
        let mut index = TileIndex::new(size);
        for coord in size.iter_coords() {
            let tile = world
                .spawn(TileBundle::new_with_terrain(coord, TerrainKind::Grass))
                .id();
            assert!(index.set(coord, tile));
        }
        world.insert_resource(index);
        world
    }
}
