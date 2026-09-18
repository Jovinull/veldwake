//! Semantic character materials, the identifier range they own, and the one
//! place a palette choice becomes a colour.
//!
//! This mirrors `veldwake-procedural`'s terrain material module deliberately,
//! and is deliberately **not** the same enum. Terrain identifiers start at 64
//! so a terrain chunk and an M2/M3 diagnostic chunk stay distinguishable;
//! characters need the same guarantee against both of those, which an extra
//! variant on somebody else's enum cannot give. The two domains therefore own
//! disjoint declared ranges and neither crate can see the other's.
//!
//! The current global allocation is recorded in
//! `docs/engineering/ARCHITECTURE.md`. The disjointness itself is proved in
//! `apps/client`, which is the lowest place both tables are visible at once.
//!
//! Unlike terrain, a character colour is not a constant: the descriptor picks
//! tones, so the palette is a compiled artefact. [`CompiledPalette`] is what a
//! renderer asks; nothing outside this module writes a character colour.

use veldwake_voxel::VoxelId;

/// First identifier of the range reserved for character content.
///
/// Chosen with headroom above terrain's `64..=74` rather than adjacent to it,
/// so a reader can tell the two domains apart at a glance in a hex dump.
pub const CHARACTER_ID_FIRST: u16 = 128;
/// One past the last identifier reserved for character content.
pub const CHARACTER_ID_END: u16 = 192;

/// Fraction of a skin tone's value that its shade band keeps.
///
/// The character style contract requires a visible separation between the two;
/// this factor is what produces it for every declared tone rather than for the
/// one that happened to be checked.
const SHADE_FACTOR: f32 = 0.66;

/// The materials the M5 humanoid can place.
///
/// Ten, which is the ceiling the character style contract sets. Every one of
/// them appears on the golden humanoid; a material with no rule behind it
/// would be an unused extension point.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CharacterMaterial {
    /// Face, hands, and the lower forearm.
    Skin,
    /// The darker band that keeps the neck and the palm from reading flat.
    SkinShade,
    /// Scalp and fringe.
    Hair,
    /// Dominant garment surface.
    TunicPrimary,
    /// The band that breaks the tunic so the torso is not one block.
    TunicSecondary,
    /// Legs above the boot.
    TrouserCloth,
    /// The horizontal break at the waist.
    Belt,
    /// Foot and lower shin.
    BootLeather,
    /// Trim; one narrow vertical stripe.
    Accent,
    /// The two face detail voxels.
    EyeDark,
}

/// Every material, in declaration order. The single list the tables and tests
/// iterate, so a new material cannot be added to one table and forgotten in
/// another.
pub const ALL_MATERIALS: [CharacterMaterial; 10] = [
    CharacterMaterial::Skin,
    CharacterMaterial::SkinShade,
    CharacterMaterial::Hair,
    CharacterMaterial::TunicPrimary,
    CharacterMaterial::TunicSecondary,
    CharacterMaterial::TrouserCloth,
    CharacterMaterial::Belt,
    CharacterMaterial::BootLeather,
    CharacterMaterial::Accent,
    CharacterMaterial::EyeDark,
];

impl CharacterMaterial {
    /// The stable voxel identifier written into a compiled body part.
    #[must_use]
    pub const fn voxel_id(self) -> VoxelId {
        VoxelId(CHARACTER_ID_FIRST + self.index())
    }

    const fn index(self) -> u16 {
        match self {
            Self::Skin => 0,
            Self::SkinShade => 1,
            Self::Hair => 2,
            Self::TunicPrimary => 3,
            Self::TunicSecondary => 4,
            Self::TrouserCloth => 5,
            Self::Belt => 6,
            Self::BootLeather => 7,
            Self::Accent => 8,
            Self::EyeDark => 9,
        }
    }

    /// Recovers the material a voxel identifier denotes, if it is one.
    ///
    /// Returns `None` for air, for terrain identifiers, and for the M2/M3
    /// diagnostic identifiers, which is what lets a renderer keep one ordered
    /// lookup across all three content domains.
    #[must_use]
    pub const fn from_voxel_id(id: VoxelId) -> Option<Self> {
        let Some(index) = id.0.checked_sub(CHARACTER_ID_FIRST) else {
            return None;
        };
        match index {
            0 => Some(Self::Skin),
            1 => Some(Self::SkinShade),
            2 => Some(Self::Hair),
            3 => Some(Self::TunicPrimary),
            4 => Some(Self::TunicSecondary),
            5 => Some(Self::TrouserCloth),
            6 => Some(Self::Belt),
            7 => Some(Self::BootLeather),
            8 => Some(Self::Accent),
            9 => Some(Self::EyeDark),
            _ => None,
        }
    }

    /// How strongly a surface of this material takes a specular highlight.
    ///
    /// Always zero. The style bible makes specular the cue that water is
    /// liquid, and nothing on a character may compete with it.
    #[must_use]
    pub const fn specular(self) -> f32 {
        0.0
    }

    /// Whether this material is bare skin rather than clothing.
    #[must_use]
    pub const fn is_skin(self) -> bool {
        matches!(self, Self::Skin | Self::SkinShade)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Skin => "skin",
            Self::SkinShade => "skin-shade",
            Self::Hair => "hair",
            Self::TunicPrimary => "tunic-primary",
            Self::TunicSecondary => "tunic-secondary",
            Self::TrouserCloth => "trouser-cloth",
            Self::Belt => "belt",
            Self::BootLeather => "boot-leather",
            Self::Accent => "accent",
            Self::EyeDark => "eye-dark",
        }
    }
}

/// Skin tone families the descriptor can name.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SkinTone {
    Fair,
    #[default]
    Tan,
    Deep,
}

/// Hair tone families the descriptor can name.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HairTone {
    Flaxen,
    #[default]
    Auburn,
    Dark,
}

/// Garment colour schemes the descriptor can name.
///
/// A scheme fills six slots at once, which is what keeps two characters from
/// the same culture looking related. Picking six colours independently is how
/// a generator produces clowns.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GarmentScheme {
    #[default]
    MossWool,
    RustLinen,
    SlateWool,
}

impl SkinTone {
    /// Every tone, in declaration order.
    pub const ALL: [Self; 3] = [Self::Fair, Self::Tan, Self::Deep];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fair => "fair",
            Self::Tan => "tan",
            Self::Deep => "deep",
        }
    }

    const fn albedo(self) -> [f32; 3] {
        match self {
            Self::Fair => [0.780, 0.585, 0.468],
            Self::Tan => [0.660, 0.470, 0.350],
            Self::Deep => [0.521, 0.360, 0.270],
        }
    }
}

impl HairTone {
    /// Every tone, in declaration order.
    pub const ALL: [Self; 3] = [Self::Flaxen, Self::Auburn, Self::Dark];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Flaxen => "flaxen",
            Self::Auburn => "auburn",
            Self::Dark => "dark",
        }
    }

    const fn albedo(self) -> [f32; 3] {
        match self {
            Self::Flaxen => [0.819, 0.700, 0.420],
            Self::Auburn => [0.341, 0.224, 0.170],
            Self::Dark => [0.235, 0.215, 0.225],
        }
    }
}

impl GarmentScheme {
    /// Every scheme, in declaration order.
    pub const ALL: [Self; 3] = [Self::MossWool, Self::RustLinen, Self::SlateWool];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MossWool => "moss-wool",
            Self::RustLinen => "rust-linen",
            Self::SlateWool => "slate-wool",
        }
    }

    /// Tunic, sleeves, trousers, belt, boots, and trim, in that order.
    ///
    /// The values are a deliberate luminance ladder rather than six chosen
    /// hues, and the ladder has one hard constraint that decided most of it:
    /// the sleeves touch bare skin at the wrist, and the three declared skin
    /// tones between them occupy `0.388` to `0.618`, so anything within eight
    /// hundredths of that whole span is unusable there. The sleeves are
    /// therefore pale, well above every skin tone, which is also what makes
    /// an arm read against the torso it hangs beside.
    const fn slots(self) -> [[f32; 3]; 6] {
        match self {
            Self::MossWool => [
                [0.196, 0.28, 0.185],
                [0.688, 0.74, 0.621],
                [0.313, 0.294, 0.25],
                [0.199, 0.161, 0.102],
                [0.208, 0.181, 0.154],
                [0.51, 0.449, 0.286],
            ],
            Self::RustLinen => [
                [0.389, 0.224, 0.165],
                [0.763, 0.714, 0.656],
                [0.309, 0.289, 0.316],
                [0.182, 0.163, 0.135],
                [0.214, 0.178, 0.171],
                [0.526, 0.444, 0.284],
            ],
            Self::SlateWool => [
                [0.222, 0.257, 0.329],
                [0.69, 0.724, 0.766],
                [0.299, 0.292, 0.312],
                [0.194, 0.161, 0.116],
                [0.195, 0.182, 0.188],
                [0.444, 0.451, 0.462],
            ],
        }
    }
}

/// The tone families a descriptor names.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PaletteChoice {
    pub skin: SkinTone,
    pub hair: HairTone,
    pub garment: GarmentScheme,
}

/// Every palette combination the descriptor can express.
#[must_use]
pub fn all_palette_choices() -> Vec<PaletteChoice> {
    let mut choices = Vec::with_capacity(27);
    for skin in SkinTone::ALL {
        for hair in HairTone::ALL {
            for garment in GarmentScheme::ALL {
                choices.push(PaletteChoice {
                    skin,
                    hair,
                    garment,
                });
            }
        }
    }
    choices
}

/// A palette choice resolved to one linear-RGB colour per material.
///
/// Immutable, cheap to copy, and the only thing a renderer needs in order to
/// turn a compiled character's voxel identifiers into vertex colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledPalette {
    albedo: [[f32; 3]; ALL_MATERIALS.len()],
}

impl CompiledPalette {
    /// Resolves the ten slots from the three tone families.
    #[must_use]
    pub fn resolve(choice: PaletteChoice) -> Self {
        let skin = choice.skin.albedo();
        let shade = [
            skin[0] * SHADE_FACTOR,
            skin[1] * SHADE_FACTOR,
            skin[2] * SHADE_FACTOR,
        ];
        let garment = choice.garment.slots();
        Self {
            albedo: [
                skin,
                shade,
                choice.hair.albedo(),
                garment[0],
                garment[1],
                garment[2],
                garment[3],
                garment[4],
                garment[5],
                [0.180, 0.170, 0.185],
            ],
        }
    }

    /// Linear-RGB albedo of one material under this palette.
    #[must_use]
    pub fn albedo(&self, material: CharacterMaterial) -> [f32; 3] {
        self.albedo[material.index() as usize]
    }

    /// Albedo and specular of a voxel identifier, if it is a character one.
    #[must_use]
    pub fn appearance(&self, id: VoxelId) -> Option<([f32; 3], f32)> {
        let material = CharacterMaterial::from_voxel_id(id)?;
        Some((self.albedo(material), material.specular()))
    }
}

/// Relative luminance under the Rec. 709 weights.
///
/// The same expression `veldwake-procedural` checks the terrain palette with.
/// It is duplicated because the two crates do not depend on each other; the
/// shared statement that matters is the style bible's, not a function pointer.
#[must_use]
pub fn luminance(albedo: [f32; 3]) -> f32 {
    albedo[1].mul_add(0.7152, albedo[0].mul_add(0.2126, albedo[2] * 0.0722))
}

/// HSV saturation: the ratio the style bible's ceiling is measured against.
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
        ALL_MATERIALS, CHARACTER_ID_END, CHARACTER_ID_FIRST, CharacterMaterial, CompiledPalette,
        all_palette_choices, luminance, saturation,
    };
    use std::collections::BTreeSet;
    use veldwake_voxel::VoxelId;

    #[test]
    fn every_material_round_trips_through_its_identifier() {
        let mut seen = BTreeSet::new();
        for material in ALL_MATERIALS {
            let id = material.voxel_id();
            assert!(
                seen.insert(id.0),
                "{} duplicates an identifier",
                material.name()
            );
            assert_eq!(CharacterMaterial::from_voxel_id(id), Some(material));
            assert!(
                (CHARACTER_ID_FIRST..CHARACTER_ID_END).contains(&id.0),
                "{} is outside the declared character range",
                material.name()
            );
        }
        assert_eq!(seen.len(), ALL_MATERIALS.len());
    }

    #[test]
    fn air_terrain_and_diagnostic_identifiers_are_not_character_materials() {
        assert_eq!(CharacterMaterial::from_voxel_id(VoxelId::AIR), None);
        // The M2/M3 diagnostic corridor writes 1, 2, and 7.
        for id in [1_u16, 2, 7] {
            assert_eq!(CharacterMaterial::from_voxel_id(VoxelId(id)), None);
        }
        // Terrain occupies 64..=74 today and the reservation runs to 127.
        for id in 64_u16..CHARACTER_ID_FIRST {
            assert_eq!(CharacterMaterial::from_voxel_id(VoxelId(id)), None);
        }
        // Nothing past the reservation is claimed either.
        assert_eq!(
            CharacterMaterial::from_voxel_id(VoxelId(CHARACTER_ID_END)),
            None
        );
        assert_eq!(CharacterMaterial::from_voxel_id(VoxelId(u16::MAX)), None);
    }

    #[test]
    fn the_reserved_range_is_wide_enough_to_be_a_reservation() {
        assert!(
            CHARACTER_ID_END - CHARACTER_ID_FIRST >= ALL_MATERIALS.len() as u16,
            "the declared range cannot hold the declared materials"
        );
    }

    #[test]
    fn every_palette_stays_inside_the_character_style_bands() {
        for choice in all_palette_choices() {
            let palette = CompiledPalette::resolve(choice);
            for material in ALL_MATERIALS {
                let albedo = palette.albedo(material);
                assert!(
                    albedo.iter().all(|channel| (0.0..=1.0).contains(channel)),
                    "{} albedo {albedo:?} leaves the unit cube",
                    material.name()
                );
                let value = luminance(albedo);
                assert!(
                    (0.16..=0.80).contains(&value),
                    "{} luminance {value} leaves the character band for {choice:?}",
                    material.name()
                );
                let ceiling = if material.is_skin()
                    || matches!(
                        material,
                        CharacterMaterial::Hair
                            | CharacterMaterial::Belt
                            | CharacterMaterial::BootLeather
                            | CharacterMaterial::EyeDark
                    ) {
                    0.55
                } else {
                    0.62
                };
                let found = saturation(albedo);
                assert!(
                    found <= ceiling,
                    "{} saturation {found} exceeds {ceiling} for {choice:?}",
                    material.name()
                );
            }
        }
    }

    #[test]
    fn skin_and_the_primary_garment_always_separate() {
        for choice in all_palette_choices() {
            let palette = CompiledPalette::resolve(choice);
            let skin = luminance(palette.albedo(CharacterMaterial::Skin));
            let tunic = luminance(palette.albedo(CharacterMaterial::TunicPrimary));
            assert!(
                (skin - tunic).abs() >= 0.12,
                "skin {skin} and tunic {tunic} are too close for {choice:?}"
            );
        }
    }

    #[test]
    fn a_character_material_never_carries_specular() {
        for material in ALL_MATERIALS {
            assert_eq!(material.specular(), 0.0, "{} shines", material.name());
        }
    }

    #[test]
    fn appearance_answers_only_for_character_identifiers() {
        let palette = CompiledPalette::resolve(super::PaletteChoice::default());
        assert!(palette.appearance(VoxelId::AIR).is_none());
        assert!(palette.appearance(VoxelId(70)).is_none());
        let skin = CharacterMaterial::Skin;
        assert_eq!(
            palette.appearance(skin.voxel_id()),
            Some((palette.albedo(skin), 0.0))
        );
    }

    #[test]
    fn material_names_are_unique() {
        let names: BTreeSet<&str> = ALL_MATERIALS.iter().map(|m| m.name()).collect();
        assert_eq!(names.len(), ALL_MATERIALS.len());
    }
}
