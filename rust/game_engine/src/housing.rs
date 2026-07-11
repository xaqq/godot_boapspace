use crate::buildings::{Building, BuildingKind};
use crate::components::Npc;
use bevy_ecs::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct House {
    capacity: usize,
    completion_order: u64,
}

impl House {
    pub const fn new(capacity: usize, completion_order: u64) -> Self {
        Self {
            capacity,
            completion_order,
        }
    }

    pub const fn capacity(self) -> usize {
        self.capacity
    }

    pub const fn completion_order(self) -> u64 {
        self.completion_order
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct HousingAssignment {
    house: Entity,
    slot: usize,
}

impl HousingAssignment {
    pub const fn new(house: Entity, slot: usize) -> Self {
        Self { house, slot }
    }

    pub const fn house(self) -> Entity {
        self.house
    }

    pub const fn slot(self) -> usize {
        self.slot
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HouseOccupancy {
    house: Entity,
    kind: BuildingKind,
    residents: Vec<Option<Entity>>,
}

impl HouseOccupancy {
    pub const fn house(&self) -> Entity {
        self.house
    }

    pub const fn kind(&self) -> BuildingKind {
        self.kind
    }

    pub fn capacity(&self) -> usize {
        self.residents.len()
    }

    pub fn occupied(&self) -> usize {
        self.residents.iter().flatten().count()
    }

    pub fn residents(&self) -> &[Option<Entity>] {
        &self.residents
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HousingSnapshot {
    houses: Vec<HouseOccupancy>,
    homeless: Vec<Entity>,
}

impl HousingSnapshot {
    pub fn houses(&self) -> &[HouseOccupancy] {
        &self.houses
    }

    pub fn homeless(&self) -> &[Entity] {
        &self.homeless
    }

    pub fn house(&self, entity: Entity) -> Option<&HouseOccupancy> {
        self.houses.iter().find(|house| house.house == entity)
    }
}

pub fn housing_snapshot(world: &World) -> HousingSnapshot {
    let mut houses = world
        .try_query::<(Entity, &Building, &House)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter(|(_, building, _)| building.kind.definition().housing_capacity().is_some())
                .map(|(entity, building, house)| {
                    (
                        entity,
                        building.kind,
                        house.capacity(),
                        house.completion_order(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    houses.sort_by_key(|(entity, _, _, completion_order)| (*completion_order, entity.index()));

    let mut occupants = houses
        .iter()
        .map(|(entity, _, capacity, _)| (*entity, vec![None; *capacity]))
        .collect::<HashMap<_, _>>();

    let mut npcs = world
        .try_query::<(Entity, &Npc, Option<&HousingAssignment>)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, _, assignment)| (entity, assignment.copied()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    npcs.sort_by_key(|(entity, _)| entity.index());

    let mut homeless = Vec::new();
    for (npc, assignment) in npcs {
        let Some(assignment) = assignment else {
            homeless.push(npc);
            continue;
        };
        let Some(slots) = occupants.get_mut(&assignment.house()) else {
            homeless.push(npc);
            continue;
        };
        let Some(slot) = slots.get_mut(assignment.slot()) else {
            homeless.push(npc);
            continue;
        };
        if slot.is_some() {
            homeless.push(npc);
        } else {
            *slot = Some(npc);
        }
    }

    HousingSnapshot {
        houses: houses
            .into_iter()
            .map(|(house, kind, _, _)| HouseOccupancy {
                house,
                kind,
                residents: occupants
                    .remove(&house)
                    .expect("collected house should have resident slots"),
            })
            .collect(),
        homeless,
    }
}

pub fn maintain_housing_assignments(
    mut commands: Commands,
    houses: Query<(Entity, &Building, &House)>,
    npcs: Query<(Entity, Option<&HousingAssignment>), With<Npc>>,
) {
    let mut ordered_houses = houses
        .iter()
        .filter(|(_, building, _)| building.kind.definition().housing_capacity().is_some())
        .map(|(entity, _, house)| (entity, *house))
        .collect::<Vec<_>>();
    ordered_houses.sort_by_key(|(entity, house)| (house.completion_order(), entity.index()));

    let mut occupied = ordered_houses
        .iter()
        .map(|(entity, house)| (*entity, vec![false; house.capacity()]))
        .collect::<HashMap<_, _>>();
    let mut ordered_npcs = npcs
        .iter()
        .map(|(entity, assignment)| (entity, assignment.copied()))
        .collect::<Vec<_>>();
    ordered_npcs.sort_by_key(|(entity, _)| entity.index());

    let mut homeless = Vec::new();
    let mut valid_assignments = HashSet::new();
    for (npc, assignment) in ordered_npcs {
        let is_valid = assignment.is_some_and(|assignment| {
            let Some(slots) = occupied.get_mut(&assignment.house()) else {
                return false;
            };
            let Some(slot) = slots.get_mut(assignment.slot()) else {
                return false;
            };
            if *slot {
                return false;
            }
            *slot = true;
            valid_assignments.insert(npc);
            true
        });
        if !is_valid {
            homeless.push(npc);
        }
    }

    for npc in homeless {
        let assignment = ordered_houses.iter().find_map(|(house, _)| {
            let slots = occupied
                .get_mut(house)
                .expect("collected house should have occupancy state");
            let slot = slots.iter().position(|occupied| !occupied)?;
            slots[slot] = true;
            Some(HousingAssignment::new(*house, slot))
        });

        if let Some(assignment) = assignment {
            commands.entity(npc).insert(assignment);
        } else if !valid_assignments.contains(&npc) {
            commands.entity(npc).remove::<HousingAssignment>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Building, BuildingFootprint};
    use crate::grid::CellCoord;
    use bevy_ecs::system::RunSystemOnce;

    fn spawn_house(world: &mut World, kind: BuildingKind, order: u64) -> Entity {
        let capacity = kind
            .definition()
            .housing_capacity()
            .expect("test kind should be a house");
        world
            .spawn((
                Building::new(kind, BuildingFootprint::new(CellCoord::new(0, 0), 1, 1)),
                House::new(capacity, order),
            ))
            .id()
    }

    #[test]
    fn assigns_npcs_to_oldest_house_and_lowest_slots() {
        let mut world = World::new();
        let newer = spawn_house(&mut world, BuildingKind::MediumHouse, 2);
        let older = spawn_house(&mut world, BuildingKind::SmallHouse, 1);
        let first = world.spawn(Npc).id();
        let second = world.spawn(Npc).id();
        let third = world.spawn(Npc).id();

        world.run_system_once(maintain_housing_assignments).unwrap();

        assert_eq!(
            world.get::<HousingAssignment>(first),
            Some(&HousingAssignment::new(older, 0))
        );
        assert_eq!(
            world.get::<HousingAssignment>(second),
            Some(&HousingAssignment::new(older, 1))
        );
        assert_eq!(
            world.get::<HousingAssignment>(third),
            Some(&HousingAssignment::new(newer, 0))
        );
    }

    #[test]
    fn preserves_valid_assignments_when_older_capacity_appears() {
        let mut world = World::new();
        let newer = spawn_house(&mut world, BuildingKind::SmallHouse, 2);
        let npc = world.spawn(Npc).id();
        world.run_system_once(maintain_housing_assignments).unwrap();
        spawn_house(&mut world, BuildingKind::SmallHouse, 1);

        world.run_system_once(maintain_housing_assignments).unwrap();

        assert_eq!(
            world.get::<HousingAssignment>(npc),
            Some(&HousingAssignment::new(newer, 0))
        );
    }

    #[test]
    fn repairs_duplicate_and_stale_assignments() {
        let mut world = World::new();
        let house = spawn_house(&mut world, BuildingKind::SmallHouse, 0);
        let first = world.spawn((Npc, HousingAssignment::new(house, 0))).id();
        let second = world.spawn((Npc, HousingAssignment::new(house, 0))).id();
        let stale = world
            .spawn((Npc, HousingAssignment::new(Entity::PLACEHOLDER, 4)))
            .id();

        world.run_system_once(maintain_housing_assignments).unwrap();

        assert_eq!(
            world.get::<HousingAssignment>(first),
            Some(&HousingAssignment::new(house, 0))
        );
        assert_eq!(
            world.get::<HousingAssignment>(second),
            Some(&HousingAssignment::new(house, 1))
        );
        assert!(world.get::<HousingAssignment>(stale).is_none());
    }

    #[test]
    fn snapshot_reports_slots_and_homeless_npcs() {
        let mut world = World::new();
        let house = spawn_house(&mut world, BuildingKind::SmallHouse, 0);
        let resident = world.spawn((Npc, HousingAssignment::new(house, 1))).id();
        let homeless = world.spawn(Npc).id();

        let snapshot = housing_snapshot(&world);

        assert_eq!(snapshot.houses().len(), 1);
        assert_eq!(snapshot.houses()[0].residents(), &[None, Some(resident)]);
        assert_eq!(snapshot.homeless(), &[homeless]);
    }

    #[test]
    fn house_despawn_reassigns_resident_to_remaining_capacity() {
        let mut world = World::new();
        let removed = spawn_house(&mut world, BuildingKind::SmallHouse, 0);
        let remaining = spawn_house(&mut world, BuildingKind::SmallHouse, 1);
        let npc = world.spawn(Npc).id();
        world.run_system_once(maintain_housing_assignments).unwrap();
        assert_eq!(
            world.get::<HousingAssignment>(npc),
            Some(&HousingAssignment::new(removed, 0))
        );

        world.despawn(removed);
        world.run_system_once(maintain_housing_assignments).unwrap();

        assert_eq!(
            world.get::<HousingAssignment>(npc),
            Some(&HousingAssignment::new(remaining, 0))
        );
    }

    #[test]
    fn npc_despawn_frees_slot_for_next_colonist() {
        let mut world = World::new();
        let house = spawn_house(&mut world, BuildingKind::SmallHouse, 0);
        let departing = world.spawn(Npc).id();
        world.run_system_once(maintain_housing_assignments).unwrap();
        world.despawn(departing);
        let replacement = world.spawn(Npc).id();

        world.run_system_once(maintain_housing_assignments).unwrap();

        assert_eq!(
            world.get::<HousingAssignment>(replacement),
            Some(&HousingAssignment::new(house, 0))
        );
    }
}
