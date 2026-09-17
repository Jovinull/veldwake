//! Procedural vegetation: a tree grammar and a placement rule.
//!
//! Vegetation here is world geometry, not entities. A tree is a set of voxels
//! written into whatever chunks it happens to overlap, so a tree standing on a
//! chunk boundary is not a special case: generating either chunk produces the
//! part of the tree that falls inside it, and neither generation knows about
//! the other.
//!
//! Placement is a lattice, never a sequential draw. Every candidate position
//! is a fixed lattice cell jittered by a hash of that cell, so:
//!
//! - the same cell always produces the same tree, in any generation order;
//! - a chunk can enumerate every tree that might reach into it by scanning the
//!   cells within one maximum tree reach of its bounds;
//! - the minimum spacing is a property of the lattice, provable rather than
//!   hoped for, because the jitter cannot exceed half the slack.

use veldwake_voxel::{CHUNK_EDGE, ChunkCoord};

use crate::{
    identity::{StreamLabel, TerrainConfig, WorldIdentity},
    material::TerrainMaterial,
    noise::{hash_2d, hash_3d, sub_hash, unit_from_hash},
    terrain::{SHORE_RISE, TerrainField},
};

/// The style bible's floor on the distance between two trunks, in voxels.
pub const MIN_TREE_SPACING: i64 = 6;
/// Shortest tree the grammar produces. Inside the style bible's 6-to-11 band;
/// the lower end is raised to eight so the canopy diameter can stay within 40
/// to 70 percent of the height without a rejection loop.
pub const MIN_TREE_HEIGHT: i64 = 8;
/// Tallest tree the grammar produces.
pub const MAX_TREE_HEIGHT: i64 = 11;
/// Largest canopy radius the grammar produces.
pub const MAX_CANOPY_RADIUS: i64 = 3;
/// How far, in voxels, a tree anchored outside a chunk can still write into it.
pub const MAX_TREE_REACH: i64 = MAX_CANOPY_RADIUS + 1;
/// Fraction of sunlit canopy voxels that take the lighter highlight material.
const HIGHLIGHT_FRACTION: f64 = 0.25;
/// Below one, the canopy stretches vertically, which reads as a crown rather
/// than as a ball.
const CANOPY_VERTICAL_SQUASH: f64 = 0.92;
/// Tallest shrub, in voxels.
const MAX_SHRUB_HEIGHT: i64 = 3;
/// Footprint edge of a shrub's base, in voxels.
///
/// One voxel wide reads as speckle rather than as a plant, which the style
/// bible rejects: a two-by-two base with a single voxel above it is the
/// smallest shape that still has a silhouette.
const SHRUB_BASE_EDGE: i64 = 2;
/// How far a shrub can reach out of the column that anchors it.
pub const MAX_SHRUB_REACH: i64 = SHRUB_BASE_EDGE - 1;

/// Sub-hash indices, named so that adding a draw cannot move an existing one.
const DRAW_JITTER_X: u64 = 0;
const DRAW_JITTER_Z: u64 = 1;
const DRAW_ACCEPT: u64 = 2;
const DRAW_HEIGHT: u64 = 3;
const DRAW_SHRUB_HEIGHT: u64 = 4;

/// Fixed stream for the canopy highlight. It is not a `StreamLabel` because it
/// is a shading variation of an already-placed voxel, not a placement decision.
const CANOPY_STREAM: u64 = 0x5645_4745_5441_5449;

/// One tree, fully described before a single voxel is written.
///
/// The descriptor exists so the grammar is inspectable and testable without a
/// chunk: a test can assert that every tree in a region is within the height
/// and canopy proportions the style bible allows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreeDescriptor {
    /// World voxel of the lowest trunk voxel.
    pub base_x: i64,
    pub base_y: i64,
    pub base_z: i64,
    /// Total height, base voxel to canopy top, inclusive.
    pub height: i64,
    /// Horizontal canopy radius in voxels.
    pub canopy_radius: i64,
    /// Trunk footprint edge: one, or two for the tallest trees.
    pub trunk_edge: i64,
}

impl TreeDescriptor {
    /// Canopy diameter as the style bible measures it.
    #[must_use]
    pub const fn canopy_diameter(&self) -> i64 {
        2 * self.canopy_radius + 1
    }

    /// World `y` of the canopy centre.
    #[must_use]
    pub const fn canopy_centre_y(&self) -> i64 {
        self.base_y + self.height - 1 - self.canopy_radius
    }

    /// Highest trunk voxel, measured from the base.
    const fn trunk_top_offset(&self) -> i64 {
        self.height - self.canopy_radius - 1
    }

    /// Inclusive world-space bounds, used to skip a tree that cannot touch a
    /// chunk before any per-voxel work happens.
    #[must_use]
    pub const fn bounds(&self) -> ([i64; 3], [i64; 3]) {
        let low = [
            self.base_x - self.canopy_radius,
            self.base_y,
            self.base_z - self.canopy_radius,
        ];
        let high = [
            self.base_x + self.canopy_radius + self.trunk_edge - 1,
            self.base_y + self.height - 1,
            self.base_z + self.canopy_radius + self.trunk_edge - 1,
        ];
        (low, high)
    }

    /// The material at a world voxel, if this tree occupies it.
    ///
    /// A pure function of the descriptor and the position, which is what makes
    /// a tree identical from whichever chunk it is written.
    #[must_use]
    pub fn material_at(&self, seed: u64, x: i64, y: i64, z: i64) -> Option<TerrainMaterial> {
        let dy = y - self.base_y;
        if dy < 0 || dy >= self.height {
            return None;
        }

        // Trunk: a one- or two-voxel column from the base into the crown, so
        // the canopy always sits on something.
        if dy <= self.trunk_top_offset()
            && (self.base_x..self.base_x + self.trunk_edge).contains(&x)
            && (self.base_z..self.base_z + self.trunk_edge).contains(&z)
        {
            return Some(TerrainMaterial::Trunk);
        }

        let centre_y = self.canopy_centre_y();
        let dx = (x - self.base_x) as f64;
        let dz = (z - self.base_z) as f64;
        let vertical = ((y - centre_y) as f64) * CANOPY_VERTICAL_SQUASH;
        let radius = self.canopy_radius as f64;
        // A quarter voxel of slack rounds the silhouette outward instead of
        // leaving the single-voxel notches a strict test produces.
        if dz.mul_add(dz, dx.mul_add(dx, vertical * vertical)) > radius.mul_add(radius, 0.25) {
            return None;
        }

        // The highlight is a hash of the world position, so it is stable and
        // never depends on iteration order. Restricting it to the sunlit upper
        // half keeps the crown reading as a lit form rather than as speckle.
        if y >= centre_y
            && unit_from_hash(hash_3d(seed, CANOPY_STREAM, x, y, z)) < HIGHLIGHT_FRACTION
        {
            Some(TerrainMaterial::FoliageHighlight)
        } else {
            Some(TerrainMaterial::Foliage)
        }
    }
}

/// One low plant: a two-by-two base with a narrower crown on top.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShrubDescriptor {
    pub base_x: i64,
    pub base_y: i64,
    pub base_z: i64,
    pub height: i64,
}

impl ShrubDescriptor {
    /// The material at a world voxel, if this shrub occupies it.
    ///
    /// The base spreads and the top does not, so the plant has a shoulder
    /// instead of being a column. Everything above the first voxel sits on the
    /// anchor column, which keeps the silhouette readable from any side.
    #[must_use]
    pub fn material_at(&self, x: i64, y: i64, z: i64) -> Option<TerrainMaterial> {
        let dy = y - self.base_y;
        if dy < 0 || dy >= self.height {
            return None;
        }
        let edge = if dy == 0 { SHRUB_BASE_EDGE } else { 1 };
        if (self.base_x..self.base_x + edge).contains(&x)
            && (self.base_z..self.base_z + edge).contains(&z)
        {
            Some(TerrainMaterial::Shrub)
        } else {
            None
        }
    }
}

/// The placement rule for one world.
#[derive(Clone, Copy, Debug)]
pub struct VegetationSystem {
    seed: u64,
    config: TerrainConfig,
    tree_stream: u64,
    shape_stream: u64,
    shrub_stream: u64,
}

impl VegetationSystem {
    #[must_use]
    pub fn new(identity: &WorldIdentity) -> Self {
        Self {
            seed: identity.seed.raw(),
            config: identity.config,
            tree_stream: identity.seed.stream(StreamLabel::TreePlacement),
            shape_stream: identity.seed.stream(StreamLabel::TreeShape),
            shrub_stream: identity.seed.stream(StreamLabel::ShrubPlacement),
        }
    }

    /// The seed the canopy highlight hashes against.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Half the slack between the lattice spacing and the minimum spacing.
    ///
    /// Capping the jitter here is what makes the spacing floor a theorem: two
    /// neighbouring anchors are `tree_spacing` apart and each moves by at most
    /// this much, so they can never come closer than the minimum.
    const fn jitter_reach(&self) -> i64 {
        let slack = self.config.tree_spacing - MIN_TREE_SPACING;
        if slack <= 0 { 0 } else { slack / 2 }
    }

    fn jitter(&self, hash: u64, draw: u64) -> i64 {
        let reach = self.jitter_reach();
        if reach == 0 {
            return 0;
        }
        let span = (2 * reach + 1).cast_unsigned();
        (sub_hash(hash, draw) % span) as i64 - reach
    }

    /// The tree anchored in one lattice cell, if that cell carries one.
    ///
    /// Every rejection is a rule from the milestone: too steep, in or beside
    /// the water, on a surface that is not soil-backed grass, outside the
    /// region, or simply not dense enough for its biome zone.
    #[must_use]
    pub fn tree_in_cell(
        &self,
        field: &TerrainField,
        cell_x: i64,
        cell_z: i64,
    ) -> Option<TreeDescriptor> {
        let hash = hash_2d(self.seed, self.tree_stream, cell_x, cell_z);
        let base_x = cell_x * self.config.tree_spacing + self.jitter(hash, DRAW_JITTER_X);
        let base_z = cell_z * self.config.tree_spacing + self.jitter(hash, DRAW_JITTER_Z);

        if !self.horizontally_inside(base_x, base_z) {
            return None;
        }

        let sample = field.sample(base_x as f64 + 0.5, base_z as f64 + 0.5);
        if sample.slope > self.config.tree_max_slope {
            return None;
        }
        if sample.is_submerged() || sample.height <= sample.water_surface + SHORE_RISE {
            return None;
        }
        if !field.surface_material(&sample).supports_vegetation() {
            return None;
        }
        if unit_from_hash(sub_hash(hash, DRAW_ACCEPT)) >= sample.zone.tree_density() {
            return None;
        }

        let shape = hash_2d(self.seed, self.shape_stream, cell_x, cell_z);
        let span = (MAX_TREE_HEIGHT - MIN_TREE_HEIGHT + 1).cast_unsigned();
        let height = MIN_TREE_HEIGHT + (sub_hash(shape, DRAW_HEIGHT) % span) as i64;
        // Taller trees carry the wider crown and the thicker trunk. Tying the
        // two together keeps every tree inside the style bible's canopy-to-
        // height band, and keeps the two-voxel trunk on the tall trees the
        // style bible allows it on.
        let tall = height >= 10;
        let tree = TreeDescriptor {
            base_x,
            base_y: sample.surface_y() + 1,
            base_z,
            height,
            canopy_radius: if tall { 3 } else { 2 },
            trunk_edge: if tall { 2 } else { 1 },
        };
        let (low, high) = tree.bounds();
        if !self.bounds_horizontally_inside(low[0], high[0], low[2], high[2]) {
            return None;
        }
        Some(tree)
    }

    /// The shrub anchored in one lattice cell, if that cell carries one.
    #[must_use]
    pub fn shrub_in_cell(
        &self,
        field: &TerrainField,
        cell_x: i64,
        cell_z: i64,
    ) -> Option<ShrubDescriptor> {
        let base_x = cell_x * self.config.shrub_spacing;
        let base_z = cell_z * self.config.shrub_spacing;
        if !self.horizontally_inside(base_x, base_z) {
            return None;
        }

        let sample = field.sample(base_x as f64 + 0.5, base_z as f64 + 0.5);
        if sample.slope > self.config.cliff_slope || sample.is_submerged() {
            return None;
        }
        // Low vegetation is allowed on the sediment margin, where a tree is
        // not: a reed at the waterline is a cue, a tree there is a mistake.
        let surface = field.surface_material(&sample);
        if !(surface.supports_vegetation() || surface == TerrainMaterial::Sediment) {
            return None;
        }

        let hash = hash_2d(self.seed, self.shrub_stream, cell_x, cell_z);
        if unit_from_hash(sub_hash(hash, DRAW_ACCEPT)) >= sample.zone.shrub_density() {
            return None;
        }
        let span = MAX_SHRUB_HEIGHT.cast_unsigned();
        let shrub = ShrubDescriptor {
            base_x,
            base_y: sample.surface_y() + 1,
            base_z,
            height: 1 + (sub_hash(hash, DRAW_SHRUB_HEIGHT) % span) as i64,
        };
        if !self.bounds_horizontally_inside(
            shrub.base_x,
            shrub.base_x + MAX_SHRUB_REACH,
            shrub.base_z,
            shrub.base_z + MAX_SHRUB_REACH,
        ) {
            return None;
        }
        Some(shrub)
    }

    /// Every tree that can write a voxel into the given chunk.
    ///
    /// The scan is one maximum reach wider than the chunk on each horizontal
    /// side, which is exactly the set of cells whose trees can overlap it.
    #[must_use]
    pub fn trees_touching(&self, field: &TerrainField, coord: ChunkCoord) -> Vec<TreeDescriptor> {
        let edge = CHUNK_EDGE as i64;
        let spacing = self.config.tree_spacing;
        let first_x = (i64::from(coord.x) * edge - MAX_TREE_REACH).div_euclid(spacing);
        let last_x = (i64::from(coord.x) * edge + edge - 1 + MAX_TREE_REACH).div_euclid(spacing);
        let first_z = (i64::from(coord.z) * edge - MAX_TREE_REACH).div_euclid(spacing);
        let last_z = (i64::from(coord.z) * edge + edge - 1 + MAX_TREE_REACH).div_euclid(spacing);

        let mut trees = Vec::new();
        for cell_z in first_z..=last_z {
            for cell_x in first_x..=last_x {
                if let Some(tree) = self.tree_in_cell(field, cell_x, cell_z) {
                    trees.push(tree);
                }
            }
        }
        trees
    }

    /// Every shrub that can write a voxel into the given chunk.
    ///
    /// One voxel of halo, because a shrub's base spreads that far out of the
    /// column that anchors it.
    #[must_use]
    pub fn shrubs_touching(&self, field: &TerrainField, coord: ChunkCoord) -> Vec<ShrubDescriptor> {
        let edge = CHUNK_EDGE as i64;
        let spacing = self.config.shrub_spacing;
        let first_x = (i64::from(coord.x) * edge - MAX_SHRUB_REACH).div_euclid(spacing);
        let last_x = (i64::from(coord.x) * edge + edge - 1).div_euclid(spacing);
        let first_z = (i64::from(coord.z) * edge - MAX_SHRUB_REACH).div_euclid(spacing);
        let last_z = (i64::from(coord.z) * edge + edge - 1).div_euclid(spacing);

        let mut shrubs = Vec::new();
        for cell_z in first_z..=last_z {
            for cell_x in first_x..=last_x {
                if let Some(shrub) = self.shrub_in_cell(field, cell_x, cell_z) {
                    shrubs.push(shrub);
                }
            }
        }
        shrubs
    }

    /// Whether any plant occupies a world voxel.
    ///
    /// Used to keep a camera out of a canopy and to answer the same question
    /// from the probe. It enumerates the lattice cells that can reach the voxel,
    /// which is the same rule the chunk generator uses, so the two cannot
    /// disagree about where a tree is.
    #[must_use]
    pub fn occupied(&self, field: &TerrainField, x: i64, y: i64, z: i64) -> bool {
        let spacing = self.config.tree_spacing;
        for cell_z in
            (z - MAX_TREE_REACH).div_euclid(spacing)..=(z + MAX_TREE_REACH).div_euclid(spacing)
        {
            for cell_x in
                (x - MAX_TREE_REACH).div_euclid(spacing)..=(x + MAX_TREE_REACH).div_euclid(spacing)
            {
                if self
                    .tree_in_cell(field, cell_x, cell_z)
                    .and_then(|tree| tree.material_at(self.seed, x, y, z))
                    .is_some()
                {
                    return true;
                }
            }
        }
        let shrub_spacing = self.config.shrub_spacing;
        for cell_z in (z - MAX_SHRUB_REACH).div_euclid(shrub_spacing)..=z.div_euclid(shrub_spacing)
        {
            for cell_x in
                (x - MAX_SHRUB_REACH).div_euclid(shrub_spacing)..=x.div_euclid(shrub_spacing)
            {
                if self
                    .shrub_in_cell(field, cell_x, cell_z)
                    .and_then(|shrub| shrub.material_at(x, y, z))
                    .is_some()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Whether a column is inside the region's horizontal footprint.
    ///
    /// Vegetation stops with the terrain: a tree hanging over the edge of the
    /// generated region would have nothing under it once the neighbouring
    /// chunk reports authoritative absence.
    fn horizontally_inside(&self, x: i64, z: i64) -> bool {
        self.bounds_horizontally_inside(x, x, z, z)
    }

    /// Whether the complete horizontal bounds of a plant fit the finite
    /// region. Plants are world geometry, so accepting only an anchor would
    /// let a canopy or shrub footprint be silently clipped by `KnownAbsent`.
    fn bounds_horizontally_inside(&self, min_x: i64, max_x: i64, min_z: i64, max_z: i64) -> bool {
        let edge = CHUNK_EDGE as i64;
        let extent = self.config.extent;
        let region_min_x = i64::from(extent.min_chunk_x) * edge;
        let region_max_x = (i64::from(extent.max_chunk_x) + 1) * edge - 1;
        let region_min_z = i64::from(extent.min_chunk_z) * edge;
        let region_max_z = (i64::from(extent.max_chunk_z) + 1) * edge - 1;
        min_x >= region_min_x
            && max_x <= region_max_x
            && min_z >= region_min_z
            && max_z <= region_max_z
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn system() -> (TerrainField, VegetationSystem) {
        let identity = WorldIdentity::golden();
        (
            TerrainField::new(&identity),
            VegetationSystem::new(&identity),
        )
    }

    /// Every tree anchored in a wide band across the golden valley, once,
    /// paired with the lattice cell that anchored it.
    fn anchored_trees() -> BTreeMap<(i64, i64), TreeDescriptor> {
        let (field, vegetation) = system();
        let spacing = TerrainConfig::golden().tree_spacing;
        let mut trees = BTreeMap::new();
        for cell_z in -380 / spacing..=380 / spacing {
            for cell_x in -380 / spacing..=380 / spacing {
                if let Some(tree) = vegetation.tree_in_cell(&field, cell_x, cell_z) {
                    assert!(
                        trees.insert((cell_x, cell_z), tree).is_none(),
                        "a cell produced two trees"
                    );
                }
            }
        }
        trees
    }

    fn trees_in_band() -> Vec<TreeDescriptor> {
        anchored_trees().into_values().collect()
    }

    #[test]
    fn the_golden_region_is_actually_vegetated() {
        let trees = trees_in_band();
        assert!(
            trees.len() > 400,
            "only {} trees in the golden region",
            trees.len()
        );
    }

    #[test]
    fn every_tree_obeys_the_style_bible_proportions() {
        for tree in trees_in_band() {
            assert!(
                (6..=11).contains(&tree.height),
                "tree height {}",
                tree.height
            );
            let diameter = tree.canopy_diameter();
            assert!((4..=7).contains(&diameter), "canopy diameter {diameter}");
            let ratio = diameter as f64 / tree.height as f64;
            assert!(
                (0.40..=0.70).contains(&ratio),
                "canopy {diameter} against height {}: {ratio}",
                tree.height
            );
            assert!(
                tree.trunk_edge == 1 || (tree.trunk_edge == 2 && tree.height >= 9),
                "a {}-high tree must not carry a {}-wide trunk",
                tree.height,
                tree.trunk_edge
            );
            assert!(tree.trunk_top_offset() >= 1, "a tree with no visible trunk");
        }
    }

    #[test]
    fn no_two_trunks_come_closer_than_the_minimum_spacing() {
        let by_cell = anchored_trees();
        for (&(cell_x, cell_z), tree) in &by_cell {
            for neighbour_z in cell_z..=cell_z + 1 {
                for neighbour_x in cell_x - 1..=cell_x + 1 {
                    if (neighbour_z, neighbour_x) <= (cell_z, cell_x) {
                        continue;
                    }
                    let Some(other) = by_cell.get(&(neighbour_x, neighbour_z)) else {
                        continue;
                    };
                    let dx = (tree.base_x - other.base_x) as f64;
                    let dz = (tree.base_z - other.base_z) as f64;
                    let distance = dz.mul_add(dz, dx * dx).sqrt();
                    assert!(
                        distance >= MIN_TREE_SPACING as f64 - 1e-9,
                        "trunks {distance} apart near cell ({cell_x}, {cell_z})"
                    );
                }
            }
        }
    }

    #[test]
    fn trees_stand_on_dry_gentle_ground_and_never_in_water() {
        let (field, _) = system();
        let config = TerrainConfig::golden();
        for tree in trees_in_band() {
            let sample = field.sample(tree.base_x as f64 + 0.5, tree.base_z as f64 + 0.5);
            assert!(
                sample.slope <= config.tree_max_slope,
                "tree on a {} slope",
                sample.slope
            );
            assert!(!sample.is_submerged(), "tree in the water");
            assert!(
                sample.height > sample.water_surface + SHORE_RISE,
                "tree on the sediment margin"
            );
            assert!(field.surface_material(&sample).supports_vegetation());
            // The base sits exactly one voxel above the surface: never buried,
            // never floating.
            assert_eq!(tree.base_y, sample.surface_y() + 1);
        }
    }

    #[test]
    fn a_tree_has_a_trunk_a_crown_and_nothing_above_it() {
        let Some(tree) = trees_in_band().into_iter().find(|tree| tree.height >= 10) else {
            panic!("the golden region should contain a tall tree");
        };

        for dy in 0..=tree.trunk_top_offset() {
            assert_eq!(
                tree.material_at(1, tree.base_x, tree.base_y + dy, tree.base_z),
                Some(TerrainMaterial::Trunk),
                "trunk gap at dy={dy}"
            );
        }
        let centre_y = tree.canopy_centre_y();
        assert!(
            tree.material_at(1, tree.base_x, centre_y, tree.base_z + tree.canopy_radius)
                .is_some_and(TerrainMaterial::is_vegetation),
            "the crown has no edge"
        );
        assert_eq!(
            tree.material_at(1, tree.base_x, tree.base_y + tree.height, tree.base_z),
            None,
            "a voxel above the declared height"
        );
        assert_eq!(
            tree.material_at(1, tree.base_x, tree.base_y - 1, tree.base_z),
            None,
            "a voxel below the base"
        );
    }

    #[test]
    fn a_tree_voxel_is_a_pure_function_of_its_position() {
        let (field, vegetation) = system();
        // A tree enumerated by two neighbouring chunks must be the same tree,
        // voxel for voxel, or a seam would show half a canopy.
        let left = vegetation.trees_touching(&field, ChunkCoord::new(0, 0, 0));
        let right = vegetation.trees_touching(&field, ChunkCoord::new(1, 0, 0));
        let shared: Vec<&TreeDescriptor> = left.iter().filter(|t| right.contains(t)).collect();
        assert!(!shared.is_empty(), "no tree spans the boundary");
        for tree in shared {
            let (low, high) = tree.bounds();
            for y in low[1]..=high[1] {
                for z in low[2]..=high[2] {
                    for x in low[0]..=high[0] {
                        assert_eq!(
                            tree.material_at(vegetation.seed(), x, y, z),
                            tree.material_at(vegetation.seed(), x, y, z)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_canopy_highlight_stays_under_the_style_bible_ceiling() {
        let (_, vegetation) = system();
        let mut canopy = 0_u32;
        let mut highlight = 0_u32;
        for tree in trees_in_band().into_iter().take(200) {
            let (low, high) = tree.bounds();
            for y in low[1]..=high[1] {
                for z in low[2]..=high[2] {
                    for x in low[0]..=high[0] {
                        match tree.material_at(vegetation.seed(), x, y, z) {
                            Some(TerrainMaterial::Foliage) => canopy += 1,
                            Some(TerrainMaterial::FoliageHighlight) => {
                                canopy += 1;
                                highlight += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        assert!(canopy > 1000, "not enough canopy sampled: {canopy}");
        let fraction = f64::from(highlight) / f64::from(canopy);
        assert!(fraction <= 0.30, "highlight fraction {fraction}");
        assert!(fraction > 0.02, "the highlight is invisible: {fraction}");
    }

    #[test]
    fn placement_does_not_depend_on_which_chunk_asks() {
        let (field, vegetation) = system();
        let coord = ChunkCoord::new(-3, 0, 2);
        assert_eq!(
            vegetation.trees_touching(&field, coord),
            vegetation.trees_touching(&field, coord)
        );
        // A tree reaching into the next chunk must be enumerated there too.
        let neighbour = vegetation.trees_touching(&field, ChunkCoord::new(-3, 0, 3));
        let boundary = 3 * CHUNK_EDGE as i64;
        for tree in vegetation.trees_touching(&field, coord) {
            if tree.bounds().1[2] >= boundary {
                assert!(neighbour.contains(&tree), "a tree vanished across a seam");
            }
        }
    }

    #[test]
    fn shrubs_are_present_low_and_grounded() {
        let (field, vegetation) = system();
        let mut total = 0;
        for coord in [
            ChunkCoord::new(0, 0, 0),
            ChunkCoord::new(1, 0, 0),
            ChunkCoord::new(-1, 0, 1),
        ] {
            let shrubs = vegetation.shrubs_touching(&field, coord);
            total += shrubs.len();
            for shrub in shrubs {
                assert!((1..=MAX_SHRUB_HEIGHT).contains(&shrub.height));
                let sample = field.sample(shrub.base_x as f64 + 0.5, shrub.base_z as f64 + 0.5);
                assert_eq!(shrub.base_y, sample.surface_y() + 1);
                assert!(!sample.is_submerged(), "a shrub in the water");
                assert_eq!(
                    shrub.material_at(shrub.base_x, shrub.base_y, shrub.base_z),
                    Some(TerrainMaterial::Shrub)
                );
                // The base spreads and the crown does not.
                assert_eq!(
                    shrub.material_at(shrub.base_x + 1, shrub.base_y, shrub.base_z + 1),
                    Some(TerrainMaterial::Shrub),
                    "a shrub with no footprint"
                );
                assert_eq!(
                    shrub.material_at(shrub.base_x + 1, shrub.base_y + 1, shrub.base_z),
                    None,
                    "a shrub that is a box"
                );
                assert_eq!(
                    shrub.material_at(shrub.base_x, shrub.base_y + shrub.height, shrub.base_z),
                    None
                );
            }
        }
        assert!(total > 5, "only {total} shrubs in three chunks");
    }

    #[test]
    fn the_canopy_covers_a_share_of_the_meadow_the_style_bible_allows() {
        // "Vegetation covers at most thirty-five percent of any meadow area."
        // Densities are not evidence of coverage, so this counts columns.
        let (field, vegetation) = system();
        let spacing = TerrainConfig::golden().tree_spacing;
        let (min_x, max_x) = (-360_i64, 360_i64);
        let (min_z, max_z) = (-120_i64, 120_i64);

        let mut covered = std::collections::BTreeSet::new();
        for cell_z in min_z.div_euclid(spacing) - 1..=max_z.div_euclid(spacing) + 1 {
            for cell_x in min_x.div_euclid(spacing) - 1..=max_x.div_euclid(spacing) + 1 {
                let Some(tree) = vegetation.tree_in_cell(&field, cell_x, cell_z) else {
                    continue;
                };
                let (low, high) = tree.bounds();
                for z in low[2]..=high[2] {
                    for x in low[0]..=high[0] {
                        covered.insert((x, z));
                    }
                }
            }
        }

        let mut grass = 0_usize;
        let mut shaded = 0_usize;
        // A coarse stride: coverage is a proportion, and sampling every fourth
        // column keeps the test under a second.
        let mut z = min_z;
        while z <= max_z {
            let mut x = min_x;
            while x <= max_x {
                let sample = field.sample(x as f64 + 0.5, z as f64 + 0.5);
                if field.surface_material(&sample).supports_vegetation() {
                    grass += 1;
                    if covered.contains(&(x, z)) {
                        shaded += 1;
                    }
                }
                x += 4;
            }
            z += 4;
        }
        assert!(grass > 5_000, "only {grass} grass columns sampled");
        let coverage = shaded as f64 / grass as f64;
        assert!(
            coverage <= 0.35,
            "the canopy covers {coverage} of the meadow"
        );
        assert!(coverage >= 0.10, "the meadow is nearly bare: {coverage}");
    }

    #[test]
    fn vegetation_stops_at_the_region_edge() {
        let (field, vegetation) = system();
        let outside = ChunkCoord::new(TerrainConfig::golden().extent.max_chunk_x + 2, 0, 0);
        assert!(vegetation.trees_touching(&field, outside).is_empty());
        assert!(vegetation.shrubs_touching(&field, outside).is_empty());
    }

    #[test]
    fn every_emittable_golden_plant_fits_the_vertical_region_before_clipping() {
        let (field, vegetation) = system();
        let extent = TerrainConfig::golden().extent;
        let edge = CHUNK_EDGE as i64;
        let min_x = i64::from(extent.min_chunk_x) * edge;
        let max_x = (i64::from(extent.max_chunk_x) + 1) * edge - 1;
        let min_z = i64::from(extent.min_chunk_z) * edge;
        let max_z = (i64::from(extent.max_chunk_z) + 1) * edge - 1;
        let min_y = i64::from(extent.min_chunk_y) * edge;
        let max_y_exclusive = (i64::from(extent.max_chunk_y) + 1) * edge;
        let mut trees = 0;
        let mut shrubs = 0;

        for cell_z in min_z.div_euclid(vegetation.config.tree_spacing)
            ..=max_z.div_euclid(vegetation.config.tree_spacing)
        {
            for cell_x in min_x.div_euclid(vegetation.config.tree_spacing)
                ..=max_x.div_euclid(vegetation.config.tree_spacing)
            {
                let Some(tree) = vegetation.tree_in_cell(&field, cell_x, cell_z) else {
                    continue;
                };
                trees += 1;
                let (low, high) = tree.bounds();
                assert!(
                    low[0] >= min_x && high[0] <= max_x && low[2] >= min_z && high[2] <= max_z,
                    "tree {tree:?} reaches x={}..={} z={}..={} outside finite region x={min_x}..={max_x} z={min_z}..={max_z} before chunk clipping",
                    low[0],
                    high[0],
                    low[2],
                    high[2]
                );
                assert!(
                    low[1] >= min_y && high[1] < max_y_exclusive,
                    "tree {tree:?} reaches y={}..={} outside [{min_y}, {max_y_exclusive}) before chunk clipping",
                    low[1],
                    high[1]
                );
            }
        }

        for cell_z in min_z.div_euclid(vegetation.config.shrub_spacing)
            ..=max_z.div_euclid(vegetation.config.shrub_spacing)
        {
            for cell_x in min_x.div_euclid(vegetation.config.shrub_spacing)
                ..=max_x.div_euclid(vegetation.config.shrub_spacing)
            {
                let Some(shrub) = vegetation.shrub_in_cell(&field, cell_x, cell_z) else {
                    continue;
                };
                shrubs += 1;
                let top = shrub.base_y + shrub.height - 1;
                let shrub_max_x = shrub.base_x + MAX_SHRUB_REACH;
                let shrub_max_z = shrub.base_z + MAX_SHRUB_REACH;
                assert!(
                    shrub.base_x >= min_x
                        && shrub_max_x <= max_x
                        && shrub.base_z >= min_z
                        && shrub_max_z <= max_z,
                    "shrub {shrub:?} reaches x={}..={shrub_max_x} z={}..={shrub_max_z} outside finite region x={min_x}..={max_x} z={min_z}..={max_z} before chunk clipping",
                    shrub.base_x,
                    shrub.base_z
                );
                assert!(
                    shrub.base_y >= min_y && top < max_y_exclusive,
                    "shrub {shrub:?} reaches y={}..={top} outside [{min_y}, {max_y_exclusive}) before chunk clipping",
                    shrub.base_y
                );
            }
        }
        assert!(
            trees > 0 && shrubs > 0,
            "the descriptor walk emitted no plants"
        );
    }
}
