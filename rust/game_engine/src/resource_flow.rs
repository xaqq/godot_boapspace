use bevy_ecs::prelude::{Entity, Resource};

use crate::resources::ResourceKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StockEndpoint {
    NaturalNode(Entity),
    CarriedResource(Entity),
    Warehouse(Entity),
    Farm(Entity),
    ForesterLodge(Entity),
    RefineryInput(Entity),
    RefineryOutput(Entity),
}

impl StockEndpoint {
    pub(crate) const fn entity(self) -> Entity {
        match self {
            Self::NaturalNode(entity)
            | Self::CarriedResource(entity)
            | Self::Warehouse(entity)
            | Self::Farm(entity)
            | Self::ForesterLodge(entity)
            | Self::RefineryInput(entity)
            | Self::RefineryOutput(entity) => entity,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SinkEndpoint {
    Blueprint(Entity),
    FoodPouch(Entity),
    Storage(Entity),
    RefineryInput(Entity),
    RefineryOutput(Entity),
}

impl SinkEndpoint {
    pub(crate) const fn entity(self) -> Entity {
        match self {
            Self::Blueprint(entity)
            | Self::FoodPouch(entity)
            | Self::Storage(entity)
            | Self::RefineryInput(entity)
            | Self::RefineryOutput(entity) => entity,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reservation {
    pub worker: Entity,
    pub source: Option<StockEndpoint>,
    pub sink: SinkEndpoint,
    pub kind: ResourceKind,
    pub amount: u32,
    pub task: Entity,
}

#[derive(Debug, Default, Resource)]
pub struct ReservationLedger {
    claims: Vec<Reservation>,
}

impl ReservationLedger {
    pub fn claims(&self) -> &[Reservation] {
        &self.claims
    }
    pub fn reserved_from(&self, source: StockEndpoint, kind: ResourceKind) -> u32 {
        self.claims
            .iter()
            .filter(|claim| claim.source == Some(source) && claim.kind == kind)
            .fold(0, |sum, claim| sum.saturating_add(claim.amount))
    }
    pub(crate) fn reserved_from_excluding_worker(
        &self,
        worker: Entity,
        source: StockEndpoint,
        kind: ResourceKind,
    ) -> u32 {
        self.claims
            .iter()
            .filter(|claim| {
                claim.worker != worker && claim.source == Some(source) && claim.kind == kind
            })
            .fold(0, |sum, claim| sum.saturating_add(claim.amount))
    }
    pub fn reserved_to(&self, sink: SinkEndpoint, kind: ResourceKind) -> u32 {
        self.claims
            .iter()
            .filter(|claim| claim.sink == sink && claim.kind == kind)
            .fold(0, |sum, claim| sum.saturating_add(claim.amount))
    }
    pub fn reserved_capacity_to(&self, sink: SinkEndpoint) -> u32 {
        self.claims
            .iter()
            .filter(|claim| claim.sink == sink)
            .fold(0, |sum, claim| sum.saturating_add(claim.amount))
    }
    pub fn claim(&mut self, reservation: Reservation) -> bool {
        if self.claims.iter().any(|claim| {
            claim.worker == reservation.worker
                || (claim.task == reservation.task
                    && matches!(reservation.sink, SinkEndpoint::RefineryOutput(_)))
        }) {
            return false;
        }
        self.claims.push(reservation);
        self.claims
            .sort_unstable_by_key(|claim| (claim.worker.to_bits(), claim.task.to_bits()));
        true
    }
    pub fn release_worker(&mut self, worker: Entity) {
        self.claims.retain(|claim| claim.worker != worker);
    }
    pub fn release_task(&mut self, task: Entity) {
        self.claims.retain(|claim| claim.task != task);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).expect("test entity index should be valid")
    }

    fn reservation(
        worker: u32,
        source: Option<StockEndpoint>,
        sink: SinkEndpoint,
        kind: ResourceKind,
        amount: u32,
        task: u32,
    ) -> Reservation {
        Reservation {
            worker: entity(worker),
            source,
            sink,
            kind,
            amount,
            task: entity(task),
        }
    }

    #[test]
    fn endpoints_project_their_entity() {
        let expected = entity(7);
        let stock_endpoints = [
            StockEndpoint::NaturalNode(expected),
            StockEndpoint::CarriedResource(expected),
            StockEndpoint::Warehouse(expected),
            StockEndpoint::Farm(expected),
            StockEndpoint::ForesterLodge(expected),
            StockEndpoint::RefineryInput(expected),
            StockEndpoint::RefineryOutput(expected),
        ];
        let sink_endpoints = [
            SinkEndpoint::Blueprint(expected),
            SinkEndpoint::FoodPouch(expected),
            SinkEndpoint::Storage(expected),
            SinkEndpoint::RefineryInput(expected),
            SinkEndpoint::RefineryOutput(expected),
        ];

        assert!(stock_endpoints
            .into_iter()
            .all(|endpoint| endpoint.entity() == expected));
        assert!(sink_endpoints
            .into_iter()
            .all(|endpoint| endpoint.entity() == expected));
    }

    #[test]
    fn ledger_aggregates_matching_sources_sinks_and_kinds() {
        let source = StockEndpoint::Warehouse(entity(100));
        let sink = SinkEndpoint::Storage(entity(101));
        let mut ledger = ReservationLedger::default();
        assert!(ledger.claim(reservation(
            1,
            Some(source),
            sink,
            ResourceKind::Wood,
            3,
            11,
        )));
        assert!(ledger.claim(reservation(
            2,
            Some(source),
            sink,
            ResourceKind::Wood,
            4,
            12,
        )));
        assert!(ledger.claim(reservation(3, None, sink, ResourceKind::Stone, 5, 13,)));

        assert_eq!(ledger.reserved_from(source, ResourceKind::Wood), 7);
        assert_eq!(
            ledger.reserved_from_excluding_worker(entity(1), source, ResourceKind::Wood),
            4
        );
        assert_eq!(ledger.reserved_to(sink, ResourceKind::Wood), 7);
        assert_eq!(ledger.reserved_to(sink, ResourceKind::Stone), 5);
        assert_eq!(ledger.reserved_capacity_to(sink), 12);
    }

    #[test]
    fn ledger_aggregates_saturate() {
        let source = StockEndpoint::Warehouse(entity(100));
        let sink = SinkEndpoint::Storage(entity(101));
        let mut ledger = ReservationLedger::default();
        assert!(ledger.claim(reservation(
            1,
            Some(source),
            sink,
            ResourceKind::Wood,
            u32::MAX,
            11,
        )));
        assert!(ledger.claim(reservation(
            2,
            Some(source),
            sink,
            ResourceKind::Wood,
            1,
            12,
        )));

        assert_eq!(ledger.reserved_from(source, ResourceKind::Wood), u32::MAX);
        assert_eq!(
            ledger.reserved_from_excluding_worker(entity(3), source, ResourceKind::Wood),
            u32::MAX
        );
        assert_eq!(ledger.reserved_to(sink, ResourceKind::Wood), u32::MAX);
        assert_eq!(ledger.reserved_capacity_to(sink), u32::MAX);
    }

    #[test]
    fn claims_are_kept_in_deterministic_worker_order() {
        let sink = SinkEndpoint::Storage(entity(100));
        let mut ledger = ReservationLedger::default();
        let mut expected = [
            (entity(30), entity(13)),
            (entity(10), entity(12)),
            (entity(20), entity(11)),
        ];
        expected.sort_unstable_by_key(|(worker, task)| (worker.to_bits(), task.to_bits()));
        for &(worker, task) in expected.iter().rev() {
            assert!(ledger.claim(Reservation {
                worker,
                source: None,
                sink,
                kind: ResourceKind::Wood,
                amount: 1,
                task,
            }));
        }

        assert_eq!(
            ledger
                .claims()
                .iter()
                .map(|claim| (claim.worker, claim.task))
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn claims_reject_duplicate_workers_and_refinery_output_tasks() {
        let storage = SinkEndpoint::Storage(entity(100));
        let refinery_output = SinkEndpoint::RefineryOutput(entity(101));
        let mut ledger = ReservationLedger::default();
        assert!(ledger.claim(reservation(1, None, storage, ResourceKind::Wood, 1, 11,)));

        assert!(!ledger.claim(reservation(1, None, storage, ResourceKind::Wood, 1, 12,)));
        assert!(!ledger.claim(reservation(
            2,
            None,
            refinery_output,
            ResourceKind::Planks,
            1,
            11,
        )));
        assert!(ledger.claim(reservation(2, None, storage, ResourceKind::Wood, 1, 11,)));
    }

    #[test]
    fn claims_can_be_released_by_worker_and_task() {
        let sink = SinkEndpoint::Storage(entity(100));
        let mut ledger = ReservationLedger::default();
        for (worker, task) in [(1, 11), (2, 12), (3, 12)] {
            assert!(ledger.claim(reservation(worker, None, sink, ResourceKind::Wood, 1, task,)));
        }

        ledger.release_worker(entity(1));
        assert_eq!(ledger.claims().len(), 2);
        assert!(ledger
            .claims()
            .iter()
            .all(|claim| claim.worker != entity(1)));

        ledger.release_task(entity(12));
        assert!(ledger.claims().is_empty());
    }
}
