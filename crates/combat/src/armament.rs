//! The player's weapon, and the one weapon that is not in a hand.
//!
//! M9's whole state model, and it is one enum value. The player holds one of
//! two weapons; the fixed exchange site holds the other; an authoritative
//! exchange swaps them. There is no inventory, no slot, no bag, no item
//! identifier and no collection of any kind, and the type is shaped so that
//! adding one would be a visible decision rather than a drift.
//!
//! **Three weapons exist in a running session, not two.** The exchange pair is
//! the player's and the site's; the adversary carries an independent
//! [`WeaponVariant::Original`] that is outside the pair and is never part of
//! any exchange. The complement invariant — "the site holds whatever the player
//! does not" — binds the pair and says nothing about the adversary.
//!
//! **Why this is authoritative and not client state.** The held weapon decides
//! which blade the hit sweep follows, which attack spec times the action, and
//! how far the aim-assist rule reaches. Letting presentation own it would let
//! presentation decide what the rules hit, which is exactly what ADR-0002 and
//! COMBAT-002 forbid. See ADR-0010.

use crate::spec::AuthoredAttack;
use crate::weapon::WeaponDescriptor;

/// Which of the two weapons in the player↔site exchange pair.
///
/// Named for **which weapon it is**, never for where it currently is. An
/// earlier draft called the starting weapon `Carried`, which stops being true
/// the moment it is left at the site.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum WeaponVariant {
    /// The weapon a session starts with, and the only one the adversary ever
    /// holds. Unchanged from M6 in every respect.
    #[default]
    Original,
    /// The weapon standing at the exchange site when a session begins.
    Found,
}

impl WeaponVariant {
    /// Both variants, in a stable order.
    pub const ALL: [Self; 2] = [Self::Original, Self::Found];

    /// The other one. There are exactly two, so this is total.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Original => Self::Found,
            Self::Found => Self::Original,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Found => "found",
        }
    }

    /// Stable index, for a fingerprint or a report.
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Original => 0,
            Self::Found => 1,
        }
    }
}

/// Which weapon the player is holding, and therefore which one the site holds.
///
/// **One value, not a pair.** With exactly two weapons in the exchange the site
/// always holds the complement of what the player holds, so storing both would
/// make "the player and the site both hold the Found weapon" a representable
/// state. [`ArmamentState::site`] derives it instead, and the two can never
/// disagree because there is only one of them.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArmamentState(WeaponVariant);

impl ArmamentState {
    /// The authored initial state of every session: the player carries the
    /// original weapon and the found one is standing at the site.
    #[must_use]
    pub const fn initial() -> Self {
        Self(WeaponVariant::Original)
    }

    /// Starting from a chosen variant, for a test that needs the other order.
    #[must_use]
    pub const fn holding(variant: WeaponVariant) -> Self {
        Self(variant)
    }

    /// What the player is holding.
    #[must_use]
    pub const fn player(self) -> WeaponVariant {
        self.0
    }

    /// What the site is holding. Derived, never stored.
    #[must_use]
    pub const fn site(self) -> WeaponVariant {
        self.0.other()
    }

    /// Swaps the two.
    ///
    /// `pub(crate)` on purpose: presentation may read an armament and may never
    /// write one. The only caller is the encounter's exchange rule.
    pub(crate) const fn exchange(&mut self) {
        self.0 = self.0.other();
    }
}

/// The exchange an encounter is configured with, if it has one.
///
/// **One optional group rather than three optional fields**, so that "a found
/// weapon with no attack" and "an attack with no weapon" are unrepresentable
/// rather than validated. Deliberately narrow: it is a weapon, the swing that
/// weapon performs, and how close a body must be to the site. It is not item
/// metadata and must not grow into any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RewardSetup {
    /// The physical object standing at the site.
    pub weapon: WeaponDescriptor,
    /// What the player does with it. Authored in seconds, compiled to ticks by
    /// the same validating constructor every other attack goes through.
    pub attack: AuthoredAttack,
    /// Planar centre-to-site distance inside which an exchange is accepted, in
    /// world units. The domain owns this number; the world only says where the
    /// site is.
    pub interact_radius: f32,
}

#[cfg(test)]
mod tests {
    use super::{ArmamentState, WeaponVariant};

    #[test]
    fn the_site_always_holds_what_the_player_does_not() {
        for variant in WeaponVariant::ALL {
            let state = ArmamentState::holding(variant);
            assert_eq!(state.player(), variant);
            assert_eq!(state.site(), variant.other());
            assert_ne!(
                state.player(),
                state.site(),
                "one weapon cannot be in two places"
            );
        }
    }

    #[test]
    fn an_exchange_is_its_own_inverse() {
        let mut state = ArmamentState::initial();
        assert_eq!(state.player(), WeaponVariant::Original);
        state.exchange();
        assert_eq!(state.player(), WeaponVariant::Found);
        assert_eq!(state.site(), WeaponVariant::Original);
        state.exchange();
        assert_eq!(
            state,
            ArmamentState::initial(),
            "two exchanges are not the identity"
        );
    }

    #[test]
    fn a_session_starts_with_the_original_weapon() {
        assert_eq!(ArmamentState::default(), ArmamentState::initial());
        assert_eq!(ArmamentState::initial().player(), WeaponVariant::Original);
        assert_eq!(ArmamentState::initial().site(), WeaponVariant::Found);
    }
}
