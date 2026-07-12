use crate::world::model_wrapper_3d::{NpcModel3D, WheelbarrowModel3D};
use crate::world::render_snapshot::NpcActivity;
use godot::classes::{INode3D, Node3D};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NpcAnimationSmokeState {
    activity: NpcActivity,
    carrying: bool,
    has_wheelbarrow: bool,
}

impl NpcAnimationSmokeState {
    const fn new(activity: NpcActivity, carrying: bool, has_wheelbarrow: bool) -> Self {
        Self {
            activity,
            carrying,
            has_wheelbarrow,
        }
    }
}

const NPC_ANIMATION_SMOKE_STATES: [NpcAnimationSmokeState; 10] = [
    NpcAnimationSmokeState::new(NpcActivity::Idle, false, false),
    NpcAnimationSmokeState::new(NpcActivity::Walk, false, false),
    NpcAnimationSmokeState::new(NpcActivity::Gather, false, false),
    NpcAnimationSmokeState::new(NpcActivity::Saw, false, false),
    NpcAnimationSmokeState::new(NpcActivity::Stonecut, false, false),
    NpcAnimationSmokeState::new(NpcActivity::Cook, false, false),
    NpcAnimationSmokeState::new(NpcActivity::Idle, true, false),
    NpcAnimationSmokeState::new(NpcActivity::Walk, true, false),
    NpcAnimationSmokeState::new(NpcActivity::Idle, false, true),
    NpcAnimationSmokeState::new(NpcActivity::Walk, false, true),
];

/// Headless-only asset integration harness. The gallery guarantees that every
/// model scene can be instantiated; this typed root additionally requests all
/// canonical animation states from every NPC wrapper and both vehicle clips.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub(crate) struct AssetSmokeTest3D {
    #[export]
    colonist: OnEditor<Gd<NpcModel3D>>,

    #[export]
    engineer: OnEditor<Gd<NpcModel3D>>,

    #[export]
    botanist: OnEditor<Gd<NpcModel3D>>,

    #[export]
    miner: OnEditor<Gd<NpcModel3D>>,

    #[export]
    scout: OnEditor<Gd<NpcModel3D>>,

    #[export]
    wheelbarrow: OnEditor<Gd<WheelbarrowModel3D>>,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for AssetSmokeTest3D {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            colonist: OnEditor::default(),
            engineer: OnEditor::default(),
            botanist: OnEditor::default(),
            miner: OnEditor::default(),
            scout: OnEditor::default(),
            wheelbarrow: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let npcs = [
            self.colonist.clone(),
            self.engineer.clone(),
            self.botanist.clone(),
            self.miner.clone(),
            self.scout.clone(),
        ];
        let mut succeeded = true;
        for mut npc in npcs {
            for state in NPC_ANIMATION_SMOKE_STATES {
                succeeded &= npc.bind_mut().set_activity(
                    state.activity,
                    state.carrying,
                    state.has_wheelbarrow,
                );
            }
        }

        let mut wheelbarrow = self.wheelbarrow.clone();
        succeeded &= wheelbarrow.bind_mut().set_rolling(false);
        succeeded &= wheelbarrow.bind_mut().set_rolling(true);

        if succeeded {
            godot_print!("AssetSmokeTest3D: all typed models and animations are ready");
        } else {
            godot_error!("AssetSmokeTest3D: one or more required animations are unavailable");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::model_wrapper_3d::npc_animation_name_3d;

    #[test]
    fn smoke_states_cover_all_ten_canonical_npc_clips_once() {
        let names = NPC_ANIMATION_SMOKE_STATES.map(|state| {
            npc_animation_name_3d(state.activity, state.carrying, state.has_wheelbarrow)
        });
        assert_eq!(
            names,
            [
                "idle",
                "walk",
                "gather",
                "saw",
                "stonecut",
                "cook",
                "carry_idle",
                "carry_walk",
                "wheelbarrow_idle",
                "wheelbarrow_walk",
            ]
        );
    }
}
