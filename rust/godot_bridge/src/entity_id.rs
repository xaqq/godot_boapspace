use bevy_ecs::prelude::Entity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InvalidBridgeEntityId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct BridgeEntityId(i64);

impl BridgeEntityId {
    pub(crate) const fn signal_value(self) -> i64 {
        self.0
    }

    pub(crate) fn entity(self) -> Entity {
        let bits = u64::try_from(self.0).expect("bridge entity IDs are non-negative");
        Entity::try_from_bits(bits).expect("bridge entity IDs contain valid Entity bits")
    }
}

impl TryFrom<Entity> for BridgeEntityId {
    type Error = InvalidBridgeEntityId;

    fn try_from(entity: Entity) -> Result<Self, Self::Error> {
        i64::try_from(entity.to_bits())
            .map(Self)
            .map_err(|_| InvalidBridgeEntityId)
    }
}

impl TryFrom<i64> for BridgeEntityId {
    type Error = InvalidBridgeEntityId;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        let bits = u64::try_from(value).map_err(|_| InvalidBridgeEntityId)?;
        Entity::try_from_bits(bits).ok_or(InvalidBridgeEntityId)?;
        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::world::World;

    #[test]
    fn entity_and_raw_values_round_trip() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();

        let bridge_id = BridgeEntityId::try_from(entity).expect("spawned entity should encode");
        let decoded = BridgeEntityId::try_from(bridge_id.signal_value())
            .expect("encoded entity should decode");

        assert_eq!(decoded, bridge_id);
        assert_eq!(decoded.entity(), entity);
    }

    #[test]
    fn negative_and_structurally_malformed_raw_values_are_rejected() {
        assert!(BridgeEntityId::try_from(-1).is_err());

        let malformed = 0_i64;
        assert!(Entity::try_from_bits(malformed as u64).is_none());
        assert!(BridgeEntityId::try_from(malformed).is_err());
    }

    #[test]
    fn valid_entity_with_sign_bit_set_is_rejected() {
        let entity = Entity::try_from_bits((1_u64 << 63) | 1)
            .expect("the sign bit is part of a valid Entity generation");

        assert!(BridgeEntityId::try_from(entity).is_err());
    }

    #[test]
    fn structurally_valid_non_live_ids_are_accepted() {
        let world = World::new();
        let entity = Entity::from_raw_u32(42).expect("index should form a valid Entity");
        assert!(world.get_entity(entity).is_err());

        let raw = i64::try_from(entity.to_bits()).expect("test entity should fit in an i64");
        let bridge_id =
            BridgeEntityId::try_from(raw).expect("liveness is not part of bridge ID validation");

        assert_eq!(bridge_id.entity(), entity);
    }
}
