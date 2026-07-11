use crate::grid::CellCoord;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlotGrowth {
    seeding_progress_ticks: u32,
    growth_ticks: Option<u32>,
}

impl PlotGrowth {
    pub(crate) const fn seedable() -> Self {
        Self {
            seeding_progress_ticks: 0,
            growth_ticks: None,
        }
    }

    pub(crate) const fn with_seeding_progress(seeding_progress_ticks: u32) -> Self {
        Self {
            seeding_progress_ticks,
            growth_ticks: None,
        }
    }

    pub(crate) const fn growing(growth_ticks: u32, seeding_duration: u32) -> Self {
        Self {
            seeding_progress_ticks: seeding_duration,
            growth_ticks: Some(growth_ticks),
        }
    }

    pub(crate) const fn seeding_progress_ticks(self) -> u32 {
        self.seeding_progress_ticks
    }

    pub(crate) const fn growth_ticks(self) -> Option<u32> {
        self.growth_ticks
    }

    pub(crate) const fn is_seedable(self, seeding_duration: u32) -> bool {
        self.growth_ticks.is_none() && self.seeding_progress_ticks < seeding_duration
    }

    pub(crate) fn advance_seeding_tick(&mut self, seeding_duration: u32) -> bool {
        if !self.is_seedable(seeding_duration) {
            return false;
        }

        self.seeding_progress_ticks = self.seeding_progress_ticks.saturating_add(1);
        if self.seeding_progress_ticks >= seeding_duration {
            self.seeding_progress_ticks = seeding_duration;
            self.growth_ticks = Some(0);
            true
        } else {
            false
        }
    }

    pub(crate) fn advance_growth_tick(&mut self, growth_duration: u32) {
        if let Some(ticks) = &mut self.growth_ticks {
            if *ticks < growth_duration {
                *ticks = ticks.saturating_add(1).min(growth_duration);
            }
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::seedable();
    }
}

pub(crate) fn cardinal_neighbors(coord: CellCoord) -> [CellCoord; 4] {
    [
        CellCoord::new(coord.x() + 1, coord.y()),
        CellCoord::new(coord.x() - 1, coord.y()),
        CellCoord::new(coord.x(), coord.y() + 1),
        CellCoord::new(coord.x(), coord.y() - 1),
    ]
}

pub(crate) fn connected_cells<'a>(
    candidates: impl IntoIterator<Item = &'a CellCoord>,
    capacity: usize,
    mut connects_to_network: impl FnMut(CellCoord, &HashSet<CellCoord>) -> bool,
) -> HashSet<CellCoord> {
    let candidates = candidates.into_iter().copied().collect::<HashSet<_>>();
    let mut connected = HashSet::new();

    while connected.len() < capacity {
        let Some(next) = candidates
            .iter()
            .copied()
            .filter(|coord| !connected.contains(coord) && connects_to_network(*coord, &connected))
            .min_by_key(|coord| (coord.y(), coord.x()))
        else {
            break;
        };
        connected.insert(next);
    }

    connected
}
