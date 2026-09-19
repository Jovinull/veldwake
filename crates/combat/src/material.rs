//! What a weapon voxel means, and the identifier range this domain owns.
//!
//! `CONTENT-001` requires every content domain to own a declared, contiguous
//! `VoxelId` range and a total round trip between its semantic material and that
//! range. Terrain holds `64..128`, characters hold `128..192`, and this domain
//! takes the next free block.
//!
//! The name is `WeaponMaterial` rather than `EquipmentMaterial` on purpose. M6
//! compiles exactly one weapon, and a general equipment domain is a claim about
//! machinery that does not exist. When a second kind of equipment arrives it can
//! widen this name, or take its own range, with its own evidence.
//!
//! **A palette is one scheme filling six slots, not three independent choices.**
//! Three independent tone families would multiply into eight combinations, and
//! the first draft of exactly that had four combinations whose blade and guard
//! landed within two hundredths of a luminance of each other — an invisible
//! material boundary, which the style rule exists to forbid. A scheme that fills
//! every slot keeps the rule a theorem the test can check exhaustively, the same
//! way a character's garment scheme fills six slots from one choice.
//!
//! **A weapon carries no specular.** The style bible makes water the liquid cue
//! and says nothing may compete with it, and a character already carries none.
//! Metal reads here through value separation instead — the polished edge and the
//! spine are almost half a unit of relative luminance apart — which costs no
//! shader change and keeps the one specular cue in the world unique.

use veldwake_voxel::VoxelId;

/// First identifier this domain owns.
pub const FIRST_WEAPON_ID: u16 = 192;
/// One past the last identifier this domain owns.
pub const WEAPON_ID_END: u16 = 224;

/// One semantic surface of a weapon.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum WeaponMaterial {
    /// The sharpened side of the blade: the brightest thing on the weapon.
    BladeEdge = 0,
    /// The flat of the blade.
    BladeBody = 1,
    /// The spine, which reads as the blade's own shadow side.
    BladeShade = 2,
    /// The crossguard.
    Guard = 3,
    /// The wrapped grip.
    Grip = 4,
    /// The pommel.
    Pommel = 5,
}

/// Every weapon material, in declaration order.
pub const ALL_WEAPON_MATERIALS: [WeaponMaterial; 6] = [
    WeaponMaterial::BladeEdge,
    WeaponMaterial::BladeBody,
    WeaponMaterial::BladeShade,
    WeaponMaterial::Guard,
    WeaponMaterial::Grip,
    WeaponMaterial::Pommel,
];

/// The material pairs that actually meet on a compiled weapon's surface.
///
/// The style rule is about boundaries a viewer can see, so this is the list the
/// palette is checked against — and `weapon.rs` proves that a compiled weapon
/// produces no face-adjacent pair outside it, so the list cannot quietly go
/// stale when the geometry changes.
pub const ADJACENT_PAIRS: [(WeaponMaterial, WeaponMaterial); 7] = [
    (WeaponMaterial::BladeEdge, WeaponMaterial::BladeBody),
    (WeaponMaterial::BladeBody, WeaponMaterial::BladeShade),
    (WeaponMaterial::BladeEdge, WeaponMaterial::Guard),
    (WeaponMaterial::BladeBody, WeaponMaterial::Guard),
    (WeaponMaterial::BladeShade, WeaponMaterial::Guard),
    (WeaponMaterial::Guard, WeaponMaterial::Grip),
    (WeaponMaterial::Grip, WeaponMaterial::Pommel),
];

impl WeaponMaterial {
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// The voxel identifier this material occupies.
    #[must_use]
    pub const fn voxel_id(self) -> VoxelId {
        VoxelId(FIRST_WEAPON_ID + self as u16)
    }

    /// The material an identifier names, if it is a weapon one.
    #[must_use]
    pub const fn from_voxel_id(id: VoxelId) -> Option<Self> {
        if id.0 < FIRST_WEAPON_ID || id.0 >= WEAPON_ID_END {
            return None;
        }
        match id.0 - FIRST_WEAPON_ID {
            0 => Some(Self::BladeEdge),
            1 => Some(Self::BladeBody),
            2 => Some(Self::BladeShade),
            3 => Some(Self::Guard),
            4 => Some(Self::Grip),
            5 => Some(Self::Pommel),
            _ => None,
        }
    }

    /// Whether this material is part of the blade.
    ///
    /// This is what the collision segment is derived from, so "which voxels are
    /// the blade" is answered by the paint rather than by a second declaration
    /// that could disagree with it.
    #[must_use]
    pub const fn is_blade(self) -> bool {
        matches!(self, Self::BladeEdge | Self::BladeBody | Self::BladeShade)
    }

    /// Specular response. Always zero; see the module note.
    #[must_use]
    pub const fn specular(self) -> f32 {
        0.0
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BladeEdge => "blade-edge",
            Self::BladeBody => "blade-body",
            Self::BladeShade => "blade-shade",
            Self::Guard => "guard",
            Self::Grip => "grip",
            Self::Pommel => "pommel",
        }
    }
}

/// One weapon's whole set of tones.
///
/// Two schemes, because one cannot prove that a palette is a palette and three
/// would be decoration. Each fills all six slots in [`ALL_WEAPON_MATERIALS`]
/// order.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum WeaponScheme {
    /// Bright steel with brass fittings and an oxblood grip.
    #[default]
    KeenSteel,
    /// Dull iron with dark fittings and a pale cord grip.
    DarkIron,
}

impl WeaponScheme {
    pub const ALL: [Self; 2] = [Self::KeenSteel, Self::DarkIron];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::KeenSteel => "keen-steel",
            Self::DarkIron => "dark-iron",
        }
    }

    /// Linear RGB for every slot, authored for the style bible's key light.
    #[must_use]
    const fn slots(self) -> [[f32; 3]; ALL_WEAPON_MATERIALS.len()] {
        match self {
            Self::KeenSteel => [
                [0.700, 0.730, 0.780],
                [0.420, 0.450, 0.500],
                [0.230, 0.250, 0.290],
                [0.460, 0.340, 0.140],
                [0.180, 0.120, 0.080],
                [0.340, 0.250, 0.100],
            ],
            Self::DarkIron => [
                [0.600, 0.620, 0.660],
                [0.340, 0.360, 0.400],
                [0.145, 0.155, 0.180],
                [0.280, 0.260, 0.220],
                [0.470, 0.440, 0.360],
                [0.360, 0.330, 0.270],
            ],
        }
    }
}

/// A scheme resolved to one colour per material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledWeaponPalette {
    albedo: [[f32; 3]; ALL_WEAPON_MATERIALS.len()],
}

impl CompiledWeaponPalette {
    #[must_use]
    pub const fn resolve(scheme: WeaponScheme) -> Self {
        Self {
            albedo: scheme.slots(),
        }
    }

    /// Linear-RGB albedo of one material under this palette.
    #[must_use]
    pub fn albedo(&self, material: WeaponMaterial) -> [f32; 3] {
        self.albedo[material.index() as usize]
    }

    /// Albedo and specular of a voxel identifier, if it is a weapon one.
    #[must_use]
    pub fn appearance(&self, id: VoxelId) -> Option<([f32; 3], f32)> {
        let material = WeaponMaterial::from_voxel_id(id)?;
        Some((self.albedo(material), material.specular()))
    }
}

/// Relative luminance under the Rec. 709 weights.
///
/// The third copy of one expression, for the reason the character crate's copy
/// already records: the shared statement that matters is the style bible's, not
/// a function pointer across a crate boundary.
#[must_use]
pub fn luminance(albedo: [f32; 3]) -> f32 {
    albedo[1].mul_add(0.7152, albedo[0].mul_add(0.2126, albedo[2] * 0.0722))
}

/// HSV saturation, the ratio the style bible's ceiling is measured against.
#[must_use]
pub fn saturation(albedo: [f32; 3]) -> f32 {
    let max = albedo[0].max(albedo[1]).max(albedo[2]);
    let min = albedo[0].min(albedo[1]).min(albedo[2]);
    if max <= f32::EPSILON {
        0.0
    } else {
        (max - min) / max
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ADJACENT_PAIRS, ALL_WEAPON_MATERIALS, CompiledWeaponPalette, FIRST_WEAPON_ID,
        WEAPON_ID_END, WeaponMaterial, WeaponScheme, luminance, saturation,
    };
    use veldwake_voxel::VoxelId;

    #[test]
    fn identifiers_round_trip_and_stay_inside_the_declared_range() {
        for material in ALL_WEAPON_MATERIALS {
            let id = material.voxel_id();
            assert!((FIRST_WEAPON_ID..WEAPON_ID_END).contains(&id.0));
            assert_eq!(WeaponMaterial::from_voxel_id(id), Some(material));
        }
        for (index, first) in ALL_WEAPON_MATERIALS.into_iter().enumerate() {
            for (other, second) in ALL_WEAPON_MATERIALS.into_iter().enumerate() {
                if index != other {
                    assert_ne!(first.voxel_id(), second.voxel_id());
                }
            }
        }
    }

    #[test]
    fn nothing_outside_the_range_is_a_weapon_material() {
        for id in [0_u16, 1, 2, 7, 63, 64, 127, 128, 191] {
            assert_eq!(WeaponMaterial::from_voxel_id(VoxelId(id)), None, "id {id}");
        }
        // The tail of the reservation is declared but unused, and must not map.
        for id in FIRST_WEAPON_ID + ALL_WEAPON_MATERIALS.len() as u16..WEAPON_ID_END {
            assert_eq!(WeaponMaterial::from_voxel_id(VoxelId(id)), None, "id {id}");
        }
        assert_eq!(WeaponMaterial::from_voxel_id(VoxelId(WEAPON_ID_END)), None);
        assert_eq!(WeaponMaterial::from_voxel_id(VoxelId(u16::MAX)), None);
    }

    #[test]
    fn the_weapon_range_is_disjoint_from_terrain_and_characters() {
        // Terrain owns 64..128 and characters own 128..192. The client proves
        // the whole table at once; this proves the half this crate declares.
        const { assert!(FIRST_WEAPON_ID >= veldwake_character::CHARACTER_ID_END) };
        for material in ALL_WEAPON_MATERIALS {
            let id = material.voxel_id();
            assert!(veldwake_character::CharacterMaterial::from_voxel_id(id).is_none());
            assert_ne!(id, VoxelId::AIR);
        }
    }

    #[test]
    fn only_the_blade_materials_are_blade() {
        assert!(WeaponMaterial::BladeEdge.is_blade());
        assert!(WeaponMaterial::BladeBody.is_blade());
        assert!(WeaponMaterial::BladeShade.is_blade());
        assert!(!WeaponMaterial::Guard.is_blade());
        assert!(!WeaponMaterial::Grip.is_blade());
        assert!(!WeaponMaterial::Pommel.is_blade());
    }

    #[test]
    fn no_weapon_material_carries_specular() {
        for material in ALL_WEAPON_MATERIALS {
            assert_eq!(
                material.specular(),
                0.0,
                "{} competes with water",
                material.name()
            );
        }
    }

    #[test]
    fn every_scheme_separates_its_face_adjacent_materials_in_value() {
        for scheme in WeaponScheme::ALL {
            let palette = CompiledWeaponPalette::resolve(scheme);
            for (first, second) in ADJACENT_PAIRS {
                let separation =
                    (luminance(palette.albedo(first)) - luminance(palette.albedo(second))).abs();
                assert!(
                    separation >= 0.08,
                    "{}: {} and {} separate by only {separation}",
                    scheme.name(),
                    first.name(),
                    second.name()
                );
            }
        }
    }

    #[test]
    fn every_scheme_stays_inside_its_value_and_saturation_bands() {
        for scheme in WeaponScheme::ALL {
            let palette = CompiledWeaponPalette::resolve(scheme);
            for material in ALL_WEAPON_MATERIALS {
                let albedo = palette.albedo(material);
                let value = luminance(albedo);
                assert!(
                    (0.10..=0.85).contains(&value),
                    "{} at {value} leaves the weapon value band under {}",
                    material.name(),
                    scheme.name()
                );
                let saturation = saturation(albedo);
                assert!(
                    saturation <= 0.72,
                    "{} at {saturation} reads as toy plastic",
                    material.name()
                );
            }
        }
    }

    #[test]
    fn a_palette_answers_by_identifier_and_refuses_foreign_ones() {
        let palette = CompiledWeaponPalette::resolve(WeaponScheme::default());
        match palette.appearance(WeaponMaterial::BladeEdge.voxel_id()) {
            Some((albedo, specular)) => {
                assert_eq!(albedo, palette.albedo(WeaponMaterial::BladeEdge));
                assert_eq!(specular, 0.0);
            }
            None => panic!("a weapon identifier must resolve"),
        }
        assert!(palette.appearance(VoxelId(64)).is_none());
        assert!(palette.appearance(VoxelId(130)).is_none());
        assert!(palette.appearance(VoxelId::AIR).is_none());
    }

    #[test]
    fn the_two_schemes_are_different_weapons_to_look_at() {
        let keen = CompiledWeaponPalette::resolve(WeaponScheme::KeenSteel);
        let dark = CompiledWeaponPalette::resolve(WeaponScheme::DarkIron);
        assert_ne!(keen, dark);
        assert!(
            luminance(keen.albedo(WeaponMaterial::BladeEdge))
                > luminance(dark.albedo(WeaponMaterial::BladeEdge))
        );
        for scheme in WeaponScheme::ALL {
            assert!(!scheme.name().is_empty());
        }
        for material in ALL_WEAPON_MATERIALS {
            assert!(!material.name().is_empty());
        }
    }

    #[test]
    fn the_blade_reads_as_metal_by_value_rather_than_by_specular() {
        // The reason the specular rule is affordable: the edge and the spine are
        // far enough apart in value to carry the material on their own.
        for scheme in WeaponScheme::ALL {
            let palette = CompiledWeaponPalette::resolve(scheme);
            let edge = luminance(palette.albedo(WeaponMaterial::BladeEdge));
            let spine = luminance(palette.albedo(WeaponMaterial::BladeShade));
            assert!(
                edge - spine >= 0.35,
                "{}: edge {edge} and spine {spine} are too close to read as metal",
                scheme.name()
            );
        }
    }

    #[test]
    fn saturation_of_a_grey_is_zero_and_of_black_is_defined() {
        assert_eq!(saturation([0.5, 0.5, 0.5]), 0.0);
        assert_eq!(saturation([0.0, 0.0, 0.0]), 0.0);
        assert!((saturation([1.0, 0.0, 0.0]) - 1.0).abs() < 1.0e-6);
    }
}
