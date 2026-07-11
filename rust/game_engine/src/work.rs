use bevy_ecs::query::QueryData;

use crate::ai::{AiConstructBuilding, AiGatherResource, AiSearchForFood};
use crate::farming::{AiHarvestField, AiSeedField};
use crate::forestry::{AiCutTreePlot, AiSeedTreePlot};
use crate::logistics::{AiBuildingHaul, AiConstructionHaul, AiFoodHaul, AiWheelbarrowRecovery};
use crate::refining::AiRefineResource;
use crate::tasks::AiConstructionLabor;

/// Read-only view of every component that represents exclusive NPC work.
///
/// Idle roaming, routes, and movement targets are deliberately absent: they
/// describe how an NPC moves when it has no work rather than owning the NPC.
#[derive(QueryData)]
pub struct NpcWorkState {
    search_for_food: Option<&'static AiSearchForFood>,
    food_haul: Option<&'static AiFoodHaul>,
    gather_resource: Option<&'static AiGatherResource>,
    construct_building: Option<&'static AiConstructBuilding>,
    construction_haul: Option<&'static AiConstructionHaul>,
    construction_labor: Option<&'static AiConstructionLabor>,
    refine_resource: Option<&'static AiRefineResource>,
    building_haul: Option<&'static AiBuildingHaul>,
    wheelbarrow_recovery: Option<&'static AiWheelbarrowRecovery>,
    seed_field: Option<&'static AiSeedField>,
    harvest_field: Option<&'static AiHarvestField>,
    seed_tree_plot: Option<&'static AiSeedTreePlot>,
    cut_tree_plot: Option<&'static AiCutTreePlot>,
}

impl NpcWorkStateItem<'_, '_> {
    pub(crate) fn is_assigned(&self) -> bool {
        self.search_for_food.is_some()
            || self.food_haul.is_some()
            || self.gather_resource.is_some()
            || self.construct_building.is_some()
            || self.construction_haul.is_some()
            || self.construction_labor.is_some()
            || self.refine_resource.is_some()
            || self.building_haul.is_some()
            || self.wheelbarrow_recovery.is_some()
            || self.seed_field.is_some()
            || self.harvest_field.is_some()
            || self.seed_tree_plot.is_some()
            || self.cut_tree_plot.is_some()
    }

    pub(crate) fn has_preemptible_refinery_supply(&self) -> bool {
        self.building_haul
            .is_some_and(|haul| haul.can_be_preempted_by_construction())
    }

    pub(crate) fn is_available_for_construction(&self) -> bool {
        self.search_for_food.is_none()
            && self.food_haul.is_none()
            && self.gather_resource.is_none()
            && self.construct_building.is_none()
            && self.construction_haul.is_none()
            && self.construction_labor.is_none()
            && self.refine_resource.is_none()
            && self
                .building_haul
                .is_none_or(|haul| haul.can_be_preempted_by_construction())
            && self.wheelbarrow_recovery.is_none()
            && self.seed_field.is_none()
            && self.harvest_field.is_none()
            && self.seed_tree_plot.is_none()
            && self.cut_tree_plot.is_none()
    }
}
