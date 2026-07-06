use crate::resources::ResourceKind;
use bevy_ecs::prelude::Component;

pub const MAX_SKILL_VALUE: u32 = 10_000;
pub const SKILL_KIND_COUNT: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum SkillKind {
    Builder = 0,
    Farmer = 1,
    Lumberjack = 2,
    Quarryman = 3,
    Forager = 4,
    Prospector = 5,
}

impl SkillKind {
    pub const ALL: [Self; SKILL_KIND_COUNT] = [
        Self::Builder,
        Self::Farmer,
        Self::Lumberjack,
        Self::Quarryman,
        Self::Forager,
        Self::Prospector,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Builder => "Builder",
            Self::Farmer => "Farmer",
            Self::Lumberjack => "Lumberjack",
            Self::Quarryman => "Quarryman",
            Self::Forager => "Forager",
            Self::Prospector => "Prospector",
        }
    }

    pub const fn for_gathered_resource(kind: ResourceKind) -> Self {
        match kind {
            ResourceKind::Wood => Self::Lumberjack,
            ResourceKind::Stone => Self::Quarryman,
            ResourceKind::Food => Self::Forager,
            ResourceKind::Gold => Self::Prospector,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillRank {
    Untrained,
    Novice,
    Apprentice,
    Journeyman,
    Skilled,
    Expert,
    Master,
    GrandMaster,
}

impl SkillRank {
    pub fn from_value(value: u32) -> Self {
        match value.min(MAX_SKILL_VALUE) {
            0 => Self::Untrained,
            1..=999 => Self::Novice,
            1000..=2499 => Self::Apprentice,
            2500..=4999 => Self::Journeyman,
            5000..=7499 => Self::Skilled,
            7500..=9499 => Self::Expert,
            9500..=9999 => Self::Master,
            10000 => Self::GrandMaster,
            _ => unreachable!(),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Untrained => "Untrained",
            Self::Novice => "Novice",
            Self::Apprentice => "Apprentice",
            Self::Journeyman => "Journeyman",
            Self::Skilled => "Skilled",
            Self::Expert => "Expert",
            Self::Master => "Master",
            Self::GrandMaster => "GrandMaster",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct NpcSkills {
    values: [u32; SKILL_KIND_COUNT],
}

impl NpcSkills {
    pub const fn zero() -> Self {
        Self {
            values: [0; SKILL_KIND_COUNT],
        }
    }

    pub fn new(values: [u32; SKILL_KIND_COUNT]) -> Self {
        let mut skills = Self::zero();
        for kind in SkillKind::ALL {
            skills.values[kind as usize] = values[kind as usize].min(MAX_SKILL_VALUE);
        }
        skills
    }

    pub const fn value(self, kind: SkillKind) -> u32 {
        self.values[kind as usize]
    }

    pub fn add_xp(&mut self, kind: SkillKind, xp: u32) {
        let value = self.value(kind);
        self.values[kind as usize] = value.saturating_add(xp).min(MAX_SKILL_VALUE);
    }

    pub fn rank(self, kind: SkillKind) -> SkillRank {
        SkillRank::from_value(self.value(kind))
    }

    pub fn percent(self, kind: SkillKind) -> u32 {
        skill_percent(self.value(kind))
    }
}

impl Default for NpcSkills {
    fn default() -> Self {
        Self::zero()
    }
}

pub fn skill_percent(value: u32) -> u32 {
    let clamped = value.min(MAX_SKILL_VALUE);
    (clamped
        .saturating_mul(100)
        .saturating_add(MAX_SKILL_VALUE / 2)
        / MAX_SKILL_VALUE)
        .min(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_skills_default_to_zero() {
        let skills = NpcSkills::default();

        for kind in SkillKind::ALL {
            assert_eq!(skills.value(kind), 0);
            assert_eq!(skills.rank(kind), SkillRank::Untrained);
            assert_eq!(skills.percent(kind), 0);
        }
    }

    #[test]
    fn skill_kind_order_and_labels_are_fixed() {
        assert_eq!(
            SkillKind::ALL.map(SkillKind::label),
            [
                "Builder",
                "Farmer",
                "Lumberjack",
                "Quarryman",
                "Forager",
                "Prospector",
            ]
        );
    }

    #[test]
    fn resource_kinds_map_to_gathering_skills() {
        assert_eq!(
            SkillKind::for_gathered_resource(ResourceKind::Wood),
            SkillKind::Lumberjack
        );
        assert_eq!(
            SkillKind::for_gathered_resource(ResourceKind::Stone),
            SkillKind::Quarryman
        );
        assert_eq!(
            SkillKind::for_gathered_resource(ResourceKind::Food),
            SkillKind::Forager
        );
        assert_eq!(
            SkillKind::for_gathered_resource(ResourceKind::Gold),
            SkillKind::Prospector
        );
    }

    #[test]
    fn skill_rank_boundaries_match_design() {
        let cases = [
            (0, SkillRank::Untrained),
            (1, SkillRank::Novice),
            (999, SkillRank::Novice),
            (1000, SkillRank::Apprentice),
            (2499, SkillRank::Apprentice),
            (2500, SkillRank::Journeyman),
            (4999, SkillRank::Journeyman),
            (5000, SkillRank::Skilled),
            (7499, SkillRank::Skilled),
            (7500, SkillRank::Expert),
            (9499, SkillRank::Expert),
            (9500, SkillRank::Master),
            (9999, SkillRank::Master),
            (10000, SkillRank::GrandMaster),
            (10001, SkillRank::GrandMaster),
        ];

        for (value, rank) in cases {
            assert_eq!(SkillRank::from_value(value), rank);
        }
    }

    #[test]
    fn skill_percent_rounds_and_clamps() {
        assert_eq!(skill_percent(0), 0);
        assert_eq!(skill_percent(49), 0);
        assert_eq!(skill_percent(50), 1);
        assert_eq!(skill_percent(149), 1);
        assert_eq!(skill_percent(150), 2);
        assert_eq!(skill_percent(9_949), 99);
        assert_eq!(skill_percent(9_950), 100);
        assert_eq!(skill_percent(10_000), 100);
        assert_eq!(skill_percent(10_001), 100);
    }

    #[test]
    fn xp_saturates_at_max_value() {
        let mut skills = NpcSkills::new([0, 0, 9_999, 0, 0, 0]);

        skills.add_xp(SkillKind::Lumberjack, 10);

        assert_eq!(skills.value(SkillKind::Lumberjack), MAX_SKILL_VALUE);
    }
}
