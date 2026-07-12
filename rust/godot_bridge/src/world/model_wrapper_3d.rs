use crate::world::render_snapshot::NpcActivity;
use godot::classes::{AnimationPlayer, BoneAttachment3D, INode3D, Node3D};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = Node3D)]
pub(crate) struct NpcModel3D {
    #[export]
    animation_player: OnEditor<Gd<AnimationPlayer>>,

    #[export]
    right_hand_attachment: OnEditor<Gd<BoneAttachment3D>>,

    #[export]
    left_hand_attachment: OnEditor<Gd<BoneAttachment3D>>,

    #[export]
    carry_attachment: OnEditor<Gd<BoneAttachment3D>>,

    #[export]
    wheelbarrow_attachment: OnEditor<Gd<BoneAttachment3D>>,

    active_animation: Option<&'static str>,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for NpcModel3D {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            animation_player: OnEditor::default(),
            right_hand_attachment: OnEditor::default(),
            left_hand_attachment: OnEditor::default(),
            carry_attachment: OnEditor::default(),
            wheelbarrow_attachment: OnEditor::default(),
            active_animation: None,
            base,
        }
    }
}

impl NpcModel3D {
    pub(crate) fn set_activity(
        &mut self,
        activity: NpcActivity,
        carrying: bool,
        has_wheelbarrow: bool,
    ) -> bool {
        let animation = npc_animation_name_3d(activity, carrying, has_wheelbarrow);
        if self.active_animation == Some(animation) {
            return true;
        }
        let name = StringName::from(animation);
        if !self.animation_player.has_animation(&name) {
            godot_error!("NpcModel3D: required animation {animation} is unavailable");
            return false;
        }
        self.animation_player.play_ex().name(&name).done();
        self.active_animation = Some(animation);
        true
    }

    pub(crate) fn carry_attachment(&self) -> Gd<BoneAttachment3D> {
        self.carry_attachment.clone()
    }

    pub(crate) fn right_hand_attachment(&self) -> Gd<BoneAttachment3D> {
        self.right_hand_attachment.clone()
    }

    pub(crate) fn wheelbarrow_attachment(&self) -> Gd<BoneAttachment3D> {
        self.wheelbarrow_attachment.clone()
    }
}

#[derive(GodotClass)]
#[class(base = Node3D)]
pub(crate) struct WheelbarrowModel3D {
    #[export]
    animation_player: OnEditor<Gd<AnimationPlayer>>,

    #[export]
    load_attachment: OnEditor<Gd<Node3D>>,

    rolling: Option<bool>,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for WheelbarrowModel3D {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            animation_player: OnEditor::default(),
            load_attachment: OnEditor::default(),
            rolling: None,
            base,
        }
    }
}

impl WheelbarrowModel3D {
    pub(crate) fn set_rolling(&mut self, rolling: bool) -> bool {
        if self.rolling == Some(rolling) {
            return true;
        }
        let animation = wheelbarrow_animation_name_3d(rolling);
        let name = StringName::from(animation);
        if self.animation_player.has_animation(&name) {
            self.animation_player.play_ex().name(&name).done();
            self.rolling = Some(rolling);
            true
        } else {
            godot_error!("WheelbarrowModel3D: required animation {animation} is unavailable");
            false
        }
    }

    pub(crate) fn load_attachment(&self) -> Gd<Node3D> {
        self.load_attachment.clone()
    }
}

pub(crate) const fn wheelbarrow_animation_name_3d(rolling: bool) -> &'static str {
    if rolling {
        "roll"
    } else {
        "idle"
    }
}

pub(crate) const fn npc_animation_name_3d(
    activity: NpcActivity,
    carrying: bool,
    has_wheelbarrow: bool,
) -> &'static str {
    match activity {
        NpcActivity::Gather => "gather",
        NpcActivity::Saw => "saw",
        NpcActivity::Stonecut => "stonecut",
        NpcActivity::Cook => "cook",
        NpcActivity::Walk if has_wheelbarrow => "wheelbarrow_walk",
        NpcActivity::Idle if has_wheelbarrow => "wheelbarrow_idle",
        NpcActivity::Walk if carrying => "carry_walk",
        NpcActivity::Idle if carrying => "carry_idle",
        NpcActivity::Walk => "walk",
        NpcActivity::Idle => "idle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_animation_precedence_uses_work_then_vehicle_then_cargo() {
        for activity in [
            NpcActivity::Gather,
            NpcActivity::Saw,
            NpcActivity::Stonecut,
            NpcActivity::Cook,
        ] {
            let expected = match activity {
                NpcActivity::Gather => "gather",
                NpcActivity::Saw => "saw",
                NpcActivity::Stonecut => "stonecut",
                NpcActivity::Cook => "cook",
                NpcActivity::Idle | NpcActivity::Walk => unreachable!(),
            };
            assert_eq!(npc_animation_name_3d(activity, true, true), expected);
        }
        assert_eq!(
            npc_animation_name_3d(NpcActivity::Walk, true, true),
            "wheelbarrow_walk"
        );
        assert_eq!(
            npc_animation_name_3d(NpcActivity::Idle, true, false),
            "carry_idle"
        );
        assert_eq!(
            npc_animation_name_3d(NpcActivity::Walk, false, false),
            "walk"
        );
        assert_eq!(
            npc_animation_name_3d(NpcActivity::Idle, false, false),
            "idle"
        );
    }

    #[test]
    fn wheelbarrow_animation_names_cover_stationary_and_rolling_states() {
        assert_eq!(wheelbarrow_animation_name_3d(false), "idle");
        assert_eq!(wheelbarrow_animation_name_3d(true), "roll");
    }
}
