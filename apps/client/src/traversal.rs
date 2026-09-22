//! Where a body may walk in a generated region, and what that makes reachable.
//!
//! Three things live here and they are deliberately layered:
//!
//! - [`TerrainWalkability`] answers `veldwake-combat`'s traversal veto from a
//!   terrain field. It is the only place the client decides that water stops a
//!   body, and it decides nothing about height.
//! - [`SurfaceGrid`] is one sampled copy of the region's walkable surface, so an
//!   offline audit can ask the movement rule about five million candidate steps
//!   without evaluating the terrain field five million times.
//! - [`audit`] walks that grid with **the movement rule itself** and reports what
//!   is reachable, what is not, and why not.
//!
//! # `GroundSampler` is not a walkability sampler
//!
//! `veldwake-character`'s contract is unchanged and must stay unchanged:
//! `surface` answers *where the visible solid surface is*, which inside a river
//! is the bed. That is exactly right for feet, IK and the pelvis, and it is the
//! wrong question for "may a body go there". The second question has its own
//! trait, [`veldwake_combat::TraversalLegality`], and this module implements it
//! beside the ground adapter rather than inside it.
//!
//! # What the audit is, and what it is not
//!
//! The audit evaluates every step from a **settled stand**: the source height is
//! the exact support surface of the source column. That is what the movement
//! rule compares, so the audit and the runtime agree by construction — but the
//! audit is still a *topological* result. It says which columns are connected
//! under the rule; it does not say that a body driven by intent, through a tick
//! loop, with separation and actions running, actually gets there. The only
//! proof of that is simulating the route, which
//! `route_is_walkable_in_a_real_encounter` does.

use std::collections::VecDeque;

use glam::Vec2;

use veldwake_character::GroundSampler;
use veldwake_combat::{MoveBlockReason, MoveRules, MovementSpec, TraversalLegality, check_move};
use veldwake_procedural::{LandmarkPlan, TerrainField, TerrainGenerator, terrain::BiomeZone};
use veldwake_voxel::{CHUNK_EDGE, ChunkCoord};

use crate::world::RegionBounds;

/// Bumped by hand when the meaning of "a body may walk here" changes.
///
/// It participates in [`route_signature`], so a rule change that leaves the
/// waypoints alone still moves the signature and forces a deliberate re-lock.
/// **Old** `1`, **new** `2`, **why**: M8. A landmark is solid, so a body may
/// not walk into one. That is a new reason for a destination to be refused,
/// and every route derived under version 1 was derived in a world where the
/// stone was not there.
pub const TRAVERSAL_RULE_VERSION: u32 = 2;

/// Horizontal room a body needs beside solid landmark stone.
///
/// The widest body the world carries, which is the adversary's, not the
/// player's: one keep-out for one world, so a wall cannot be solid for one
/// body and open for the other. `the_keep_out_is_the_widest_body_the_world_carries`
/// compiles both rigs and asserts this is their capsule radius rounded up.
pub const BODY_KEEP_OUT_RADIUS: f64 = 0.86;

/// Vertical room a body needs above the surface it stands on.
///
/// Also the widest body's, and also asserted rather than assumed. It is what
/// makes a gate passable: the stone over the opening is far higher than this,
/// so the columns under it are not keep-out at all.
pub const BODY_KEEP_OUT_HEIGHT: f64 = 2.84;

/// Whether a landmark fills any voxel a body standing on `floor` would need.
///
/// The whole of M8's contribution to movement. It is deliberately not a
/// collision engine: it asks one question about one destination column, at
/// the height a body actually occupies, exactly as the water veto does.
/// Nothing here walks on a landmark — a roof is not ground and
/// `GroundSampler` is untouched.
fn landmark_in_the_way(plan: &LandmarkPlan, x: f64, z: f64, floor: f64) -> bool {
    let radius = BODY_KEEP_OUT_RADIUS;
    let (Some(low_x), Some(high_x)) = (floor_to_i64(x - radius), floor_to_i64(x + radius)) else {
        return false;
    };
    let (Some(low_z), Some(high_z)) = (floor_to_i64(z - radius), floor_to_i64(z + radius)) else {
        return false;
    };
    let top = floor + BODY_KEEP_OUT_HEIGHT;
    for column_z in low_z..=high_z {
        for column_x in low_x..=high_x {
            // The body is a disc, not a square: a column whose nearest point
            // is further than the radius is not touched.
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            let (cx, cz) = (column_x as f64, column_z as f64);
            let dx = (cx - x).max(x - (cx + 1.0)).max(0.0);
            let dz = (cz - z).max(z - (cz + 1.0)).max(0.0);
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            let Some((filled_low, filled_high)) = plan.column(column_x, column_z) else {
                continue;
            };
            #[expect(clippy::cast_precision_loss, reason = "region heights")]
            let (stone_low, stone_high) = (filled_low as f64, (filled_high + 1) as f64);
            if stone_low < top && stone_high > floor {
                return true;
            }
        }
    }
    false
}

/// The traversal veto of one world.
///
/// Water blocks, the region's edge blocks, a landmark blocks, and nothing else
/// does — height is the ground sampler's business and this type never reports
/// one.
#[derive(Clone, Copy, Debug)]
pub struct TerrainWalkability<'a> {
    field: &'a TerrainField,
    plan: &'a LandmarkPlan,
    bounds: RegionBounds,
}

impl<'a> TerrainWalkability<'a> {
    #[must_use]
    pub fn new(generator: &'a TerrainGenerator) -> Self {
        Self {
            field: generator.field(),
            plan: generator.landmarks(),
            bounds: RegionBounds::of(generator),
        }
    }

    #[must_use]
    pub const fn bounds(&self) -> RegionBounds {
        self.bounds
    }
}

impl TraversalLegality for TerrainWalkability<'_> {
    fn walkable(&self, x: f64, z: f64) -> bool {
        if !self.bounds.contains(x, z) {
            return false;
        }
        // `has_water_voxel` is the generator's own predicate for "this column
        // contains water the renderer draws". The continuous relation
        // `is_submerged` is a different question and would block columns a
        // viewer sees as dry ground; INVARIANTS.md TRAVERSE-001 is that
        // distinction written down.
        let sample = self.field.sample(x, z);
        if sample.has_water_voxel() {
            return false;
        }
        #[expect(clippy::cast_precision_loss, reason = "region heights")]
        let floor = sample.surface_y().saturating_add(1) as f64;
        !landmark_in_the_way(self.plan, x, z, floor)
    }
}

/// The centre of the column containing a world coordinate pair.
///
/// Every rule evaluated on a grid is evaluated here, so a column is judged at
/// one representative point rather than at whichever edge a caller happened to
/// pass.
#[must_use]
pub fn column_centre(x: i64, z: i64) -> Vec2 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "region coordinates are hundreds, exactly representable in f32"
    )]
    Vec2::new(x as f32 + 0.5, z as f32 + 0.5)
}

/// One sampled copy of a region's walkable surface.
///
/// Debug prints the shape, never the cells: six hundred and forty thousand
/// heights are not a diagnostic.
///
/// Built once, read many times. Every entry comes from the same
/// `TerrainField` the runtime adapters read, and
/// `the_cached_grid_agrees_with_the_runtime_adapters` asserts the two answer
/// identically at column centres — without which this would be a second,
/// silently drifting implementation of the world.
pub struct SurfaceGrid {
    bounds: RegionBounds,
    min_x: i64,
    min_z: i64,
    columns_x: usize,
    columns_z: usize,
    /// Support surface of each column, the face a sole rests on. `NO_GROUND`
    /// where the region does not reach.
    support: Vec<i32>,
    /// Whether the generator writes a water voxel in the column.
    water: Vec<bool>,
    /// Whether a landmark fills the room a body needs in the column.
    blocked: Vec<bool>,
    /// Biome zone of the column, for reporting what a barrier cuts off.
    zone: Vec<BiomeZone>,
}

impl std::fmt::Debug for SurfaceGrid {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SurfaceGrid")
            .field("columns_x", &self.columns_x)
            .field("columns_z", &self.columns_z)
            .field("min_x", &self.min_x)
            .field("min_z", &self.min_z)
            .finish_non_exhaustive()
    }
}

/// Sentinel for a column the region does not contain.
const NO_GROUND: i32 = i32::MIN;

impl SurfaceGrid {
    /// Samples the whole region, chunk column by chunk column.
    ///
    /// Deterministic in every respect: the chunk order is the extent's own, the
    /// column order inside a chunk is the generator's own, and every allocation
    /// is made once at its final size.
    #[must_use]
    pub fn sample(generator: &TerrainGenerator) -> Self {
        let plan = generator.landmarks();
        let extent = generator.identity().config.extent;
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let min_x = i64::from(extent.min_chunk_x) * edge;
        let min_z = i64::from(extent.min_chunk_z) * edge;
        let columns_x =
            usize::try_from((i64::from(extent.max_chunk_x) + 1) * edge - min_x).unwrap_or_default();
        let columns_z =
            usize::try_from((i64::from(extent.max_chunk_z) + 1) * edge - min_z).unwrap_or_default();
        let cells = columns_x.saturating_mul(columns_z);

        let mut grid = Self {
            bounds: RegionBounds::of(generator),
            min_x,
            min_z,
            columns_x,
            columns_z,
            support: vec![NO_GROUND; cells],
            water: vec![false; cells],
            blocked: vec![false; cells],
            zone: vec![BiomeZone::Meadow; cells],
        };

        for chunk_z in extent.min_chunk_z..=extent.max_chunk_z {
            for chunk_x in extent.min_chunk_x..=extent.max_chunk_x {
                let samples = generator.column_samples(ChunkCoord::new(chunk_x, 0, chunk_z));
                for local_z in 0..CHUNK_EDGE {
                    for local_x in 0..CHUNK_EDGE {
                        let Some(sample) = samples.get(local_z * CHUNK_EDGE + local_x) else {
                            continue;
                        };
                        let world_x = i64::from(chunk_x) * edge + local_x as i64;
                        let world_z = i64::from(chunk_z) * edge + local_z as i64;
                        let Some(index) = grid.index_of(world_x, world_z) else {
                            continue;
                        };
                        // The same arithmetic `TerrainGround::surface` uses: the
                        // face above the topmost solid voxel.
                        let support = sample.surface_y().saturating_add(1);
                        grid.support[index] = i32::try_from(support).unwrap_or(i32::MAX);
                        grid.water[index] = sample.has_water_voxel();
                        grid.zone[index] = sample.zone;
                        // The same rule the runtime veto applies, asked at
                        // the column centre, which is where every grid rule
                        // is asked.
                        let centre = column_centre(world_x, world_z);
                        #[expect(clippy::cast_precision_loss, reason = "region heights")]
                        let floor = support as f64;
                        grid.blocked[index] = landmark_in_the_way(
                            plan,
                            f64::from(centre.x),
                            f64::from(centre.y),
                            floor,
                        );
                    }
                }
            }
        }
        grid
    }

    #[must_use]
    pub const fn columns_x(&self) -> usize {
        self.columns_x
    }

    #[must_use]
    pub const fn columns_z(&self) -> usize {
        self.columns_z
    }

    /// How many columns the grid holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.support.len()
    }

    /// Clippy asks for this beside [`Self::len`]; nothing needs it, because a
    /// region with no columns is not a region.
    #[expect(dead_code, reason = "required beside `len`, and never called")]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.support.is_empty()
    }

    /// Linear index of a column, or `None` outside the grid.
    #[must_use]
    pub fn index_of(&self, x: i64, z: i64) -> Option<usize> {
        let ix = usize::try_from(x.checked_sub(self.min_x)?).ok()?;
        let iz = usize::try_from(z.checked_sub(self.min_z)?).ok()?;
        if ix >= self.columns_x || iz >= self.columns_z {
            return None;
        }
        Some(iz * self.columns_x + ix)
    }

    /// World column of a linear index.
    #[must_use]
    pub fn column_of(&self, index: usize) -> Option<(i64, i64)> {
        if index >= self.support.len() || self.columns_x == 0 {
            return None;
        }
        let ix = index % self.columns_x;
        let iz = index / self.columns_x;
        Some((
            self.min_x
                .saturating_add(i64::try_from(ix).unwrap_or_default()),
            self.min_z
                .saturating_add(i64::try_from(iz).unwrap_or_default()),
        ))
    }

    /// Support height of a column, or `None` where the region does not reach.
    #[must_use]
    pub fn support_at(&self, x: i64, z: i64) -> Option<i32> {
        let index = self.index_of(x, z)?;
        let height = self.support[index];
        (height != NO_GROUND).then_some(height)
    }

    /// Whether the column holds a water voxel.
    #[must_use]
    pub fn has_water(&self, x: i64, z: i64) -> bool {
        self.index_of(x, z).is_some_and(|index| self.water[index])
    }

    /// Whether a landmark fills the room a body needs in this column.
    #[must_use]
    pub fn blocked_by_landmark(&self, x: i64, z: i64) -> bool {
        self.index_of(x, z).is_some_and(|index| self.blocked[index])
    }

    /// Whether a body could stand in this column at all: inside the region,
    /// not in water, and not inside a landmark.
    #[must_use]
    pub fn standable(&self, x: i64, z: i64) -> bool {
        self.support_at(x, z).is_some() && !self.has_water(x, z) && !self.blocked_by_landmark(x, z)
    }

    #[must_use]
    pub fn zone_at(&self, x: i64, z: i64) -> Option<BiomeZone> {
        self.index_of(x, z).map(|index| self.zone[index])
    }

    /// The ground adapter over this grid.
    #[must_use]
    pub const fn ground(&self) -> GridGround<'_> {
        GridGround { grid: self }
    }

    /// The traversal veto over this grid.
    #[must_use]
    pub const fn legality(&self) -> GridWalkability<'_> {
        GridWalkability { grid: self }
    }
}

/// `GroundSampler` backed by a sampled grid rather than by the field.
///
/// Exists for cost alone: the audit asks the movement rule about roughly five
/// million candidate steps, and each one samples the ground twice. It answers
/// identically to [`crate::character::TerrainGround`] at every column centre,
/// which is asserted rather than assumed.
#[derive(Clone, Copy, Debug)]
pub struct GridGround<'a> {
    grid: &'a SurfaceGrid,
}

impl GroundSampler for GridGround<'_> {
    fn surface(&self, x: f64, z: f64) -> Option<f64> {
        if !self.grid.bounds.contains(x, z) {
            return None;
        }
        let height = self.grid.support_at(floor_to_i64(x)?, floor_to_i64(z)?)?;
        Some(f64::from(height))
    }
}

/// `TraversalLegality` backed by a sampled grid.
#[derive(Clone, Copy, Debug)]
pub struct GridWalkability<'a> {
    grid: &'a SurfaceGrid,
}

impl TraversalLegality for GridWalkability<'_> {
    fn walkable(&self, x: f64, z: f64) -> bool {
        if !self.grid.bounds.contains(x, z) {
            return false;
        }
        let (Some(x), Some(z)) = (floor_to_i64(x), floor_to_i64(z)) else {
            return false;
        };
        !self.grid.has_water(x, z) && !self.grid.blocked_by_landmark(x, z)
    }
}

/// Floors a coordinate into a column index, rejecting anything a cast would
/// silently mangle.
fn floor_to_i64(value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }
    let floored = value.floor();
    if floored < -(2.0_f64).powi(62) || floored > (2.0_f64).powi(62) {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the range was just checked against a bound well inside i64"
    )]
    Some(floored as i64)
}

/// The eight steps a body can take between columns, in a fixed order.
///
/// Eight rather than four because the client hands the rules a diagonal
/// intent and `try_move` tests the whole diagonal displacement before it tries
/// either axis. A four-neighbour audit would report less than the game allows.
/// The order is fixed so a breadth-first walk is reproducible.
const NEIGHBOURS: [(i64, i64); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// What one reachability audit found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReachabilityReport {
    pub start: (i64, i64),
    pub columns_total: usize,
    /// Columns inside the region, whatever is in them.
    pub columns_with_ground: usize,
    /// Columns a body could stand in: inside the region and not water.
    pub columns_standable: usize,
    /// Columns reachable from the start.
    pub forward: usize,
    /// Columns from which the start is reachable.
    pub reverse: usize,
    /// Columns reachable from the start **and** able to return to it.
    pub bidirectional: usize,
    /// Columns you can walk to and not walk back from.
    pub one_way: usize,
    /// Sizes of the largest components of the symmetric relation, descending.
    ///
    /// Symmetric means both directions are legal between neighbours. These are
    /// not strongly connected components of the directed graph, and the
    /// difference is why `one_way` is reported separately.
    pub component_sizes: Vec<usize>,
    /// Refusals encountered while expanding reachable columns, by first cause.
    pub barriers: [u64; MoveBlockReason::ALL.len()],
    /// Reachable columns in the highland zones.
    pub highland_reachable: usize,
    /// Total highland columns a body could otherwise stand in.
    pub highland_standable: usize,
    /// The nearest reachable highland column and its step distance.
    pub nearest_highland: Option<(i64, i64, u32)>,
    /// Step distance from the start to every column, for route derivation.
    pub distance: Vec<u32>,
    /// Predecessor of every reached column, for route derivation.
    predecessor: Vec<u32>,
}

/// Sentinel distance for "not reached".
const UNREACHED: u32 = u32::MAX;

impl ReachabilityReport {
    /// Whether any highland column is reachable on foot from the start.
    #[must_use]
    pub const fn highland_is_reachable(&self) -> bool {
        self.highland_reachable > 0
    }

    /// Step distance from the start, or `None` if the column was not reached.
    #[must_use]
    pub fn distance_to(&self, grid: &SurfaceGrid, x: i64, z: i64) -> Option<u32> {
        let index = grid.index_of(x, z)?;
        let distance = *self.distance.get(index)?;
        (distance != UNREACHED).then_some(distance)
    }

    /// The column path from the start to a column, inclusive at both ends.
    #[must_use]
    pub fn path_to(&self, grid: &SurfaceGrid, x: i64, z: i64) -> Option<Vec<(i64, i64)>> {
        let mut index = grid.index_of(x, z)?;
        if *self.distance.get(index)? == UNREACHED {
            return None;
        }
        let mut path = Vec::new();
        loop {
            path.push(grid.column_of(index)?);
            let previous = *self.predecessor.get(index)?;
            if previous == UNREACHED {
                break;
            }
            let previous = usize::try_from(previous).ok()?;
            if previous == index {
                break;
            }
            index = previous;
        }
        path.reverse();
        Some(path)
    }
}

/// Walks the whole region with the movement rule and reports what it found.
///
/// The rule is `veldwake_combat::check_move`, called rather than reimplemented,
/// so the audit cannot drift from the game. What the audit supplies is the data
/// the rule reads, cached.
#[must_use]
pub fn audit(grid: &SurfaceGrid, movement: &MovementSpec, start: (i64, i64)) -> ReachabilityReport {
    let ground = grid.ground();
    let legality = grid.legality();
    let rules = MoveRules {
        movement,
        arena: None,
        ground: Some(&ground),
        legality: Some(&legality),
    };

    let cells = grid.len();
    let mut distance = vec![UNREACHED; cells];
    let mut predecessor = vec![UNREACHED; cells];
    let mut barriers = [0_u64; MoveBlockReason::ALL.len()];

    // Forward: everything the start can reach.
    let mut queue: VecDeque<u32> = VecDeque::with_capacity(cells);
    if let Some(index) = grid.index_of(start.0, start.1)
        && grid.standable(start.0, start.1)
    {
        distance[index] = 0;
        queue.push_back(u32::try_from(index).unwrap_or(UNREACHED));
    }
    while let Some(raw) = queue.pop_front() {
        let Ok(index) = usize::try_from(raw) else {
            continue;
        };
        let Some((x, z)) = grid.column_of(index) else {
            continue;
        };
        let from = column_centre(x, z);
        let step = distance[index].saturating_add(1);
        for (dx, dz) in NEIGHBOURS {
            let (nx, nz) = (x.saturating_add(dx), z.saturating_add(dz));
            let Some(neighbour) = grid.index_of(nx, nz) else {
                continue;
            };
            match check_move(&rules, from, column_centre(nx, nz)) {
                Ok(()) => {
                    if distance[neighbour] == UNREACHED {
                        distance[neighbour] = step;
                        predecessor[neighbour] = raw;
                        queue.push_back(u32::try_from(neighbour).unwrap_or(UNREACHED));
                    }
                }
                Err(reason) => {
                    let slot = MoveBlockReason::ALL
                        .iter()
                        .position(|candidate| *candidate == reason)
                        .unwrap_or_default();
                    barriers[slot] = barriers[slot].saturating_add(1);
                }
            }
        }
    }

    // Reverse: everything that can reach the start, by expanding the
    // transposed relation from the same seed.
    let mut reverse = vec![false; cells];
    queue.clear();
    if let Some(index) = grid.index_of(start.0, start.1)
        && grid.standable(start.0, start.1)
    {
        reverse[index] = true;
        queue.push_back(u32::try_from(index).unwrap_or(UNREACHED));
    }
    while let Some(raw) = queue.pop_front() {
        let Ok(index) = usize::try_from(raw) else {
            continue;
        };
        let Some((x, z)) = grid.column_of(index) else {
            continue;
        };
        let here = column_centre(x, z);
        for (dx, dz) in NEIGHBOURS {
            let (nx, nz) = (x.saturating_add(dx), z.saturating_add(dz));
            let Some(neighbour) = grid.index_of(nx, nz) else {
                continue;
            };
            if reverse[neighbour] {
                continue;
            }
            // The transposed edge: can the neighbour step to here?
            if check_move(&rules, column_centre(nx, nz), here).is_ok() {
                reverse[neighbour] = true;
                queue.push_back(u32::try_from(neighbour).unwrap_or(UNREACHED));
            }
        }
    }

    let mut forward_count = 0_usize;
    let mut reverse_count = 0_usize;
    let mut bidirectional = 0_usize;
    let mut one_way = 0_usize;
    let mut columns_with_ground = 0_usize;
    let mut columns_standable = 0_usize;
    let mut highland_reachable = 0_usize;
    let mut highland_standable = 0_usize;
    let mut nearest_highland: Option<(i64, i64, u32)> = None;

    for index in 0..cells {
        let Some((x, z)) = grid.column_of(index) else {
            continue;
        };
        let has_ground = grid.support_at(x, z).is_some();
        let standable = grid.standable(x, z);
        if has_ground {
            columns_with_ground += 1;
        }
        if standable {
            columns_standable += 1;
        }
        let highland = matches!(
            grid.zone_at(x, z),
            Some(BiomeZone::Highland | BiomeZone::RockyRidge)
        );
        if highland && standable {
            highland_standable += 1;
        }
        let reached = distance[index] != UNREACHED;
        if reached {
            forward_count += 1;
            if highland {
                highland_reachable += 1;
                let step = distance[index];
                if nearest_highland.is_none_or(|(_, _, best)| step < best) {
                    nearest_highland = Some((x, z, step));
                }
            }
        }
        if reverse[index] {
            reverse_count += 1;
        }
        match (reached, reverse[index]) {
            (true, true) => bidirectional += 1,
            (true, false) => one_way += 1,
            _ => {}
        }
    }

    ReachabilityReport {
        start,
        columns_total: cells,
        columns_with_ground,
        columns_standable,
        forward: forward_count,
        reverse: reverse_count,
        bidirectional,
        one_way,
        component_sizes: symmetric_component_sizes(grid, &rules),
        barriers,
        highland_reachable,
        highland_standable,
        nearest_highland,
        distance,
        predecessor,
    }
}

/// Sizes of the components of the *symmetric* step relation, descending.
///
/// A component here is a set of columns that can all reach each other using
/// steps that are legal in both directions. It deliberately ignores one-way
/// drops, which the report counts separately: treating a cliff a body can fall
/// down but not climb as a connection would overstate how navigable a region is.
fn symmetric_component_sizes(grid: &SurfaceGrid, rules: &MoveRules<'_>) -> Vec<usize> {
    let cells = grid.len();
    let mut label = vec![false; cells];
    let mut sizes = Vec::new();
    let mut queue: VecDeque<u32> = VecDeque::with_capacity(cells);

    for seed in 0..cells {
        if label[seed] {
            continue;
        }
        let Some((sx, sz)) = grid.column_of(seed) else {
            continue;
        };
        if !grid.standable(sx, sz) {
            continue;
        }
        label[seed] = true;
        queue.clear();
        queue.push_back(u32::try_from(seed).unwrap_or(UNREACHED));
        let mut size = 0_usize;
        while let Some(raw) = queue.pop_front() {
            let Ok(index) = usize::try_from(raw) else {
                continue;
            };
            let Some((x, z)) = grid.column_of(index) else {
                continue;
            };
            size += 1;
            let here = column_centre(x, z);
            for (dx, dz) in NEIGHBOURS {
                let (nx, nz) = (x.saturating_add(dx), z.saturating_add(dz));
                let Some(neighbour) = grid.index_of(nx, nz) else {
                    continue;
                };
                if label[neighbour] {
                    continue;
                }
                let there = column_centre(nx, nz);
                if check_move(rules, here, there).is_ok() && check_move(rules, there, here).is_ok()
                {
                    label[neighbour] = true;
                    queue.push_back(u32::try_from(neighbour).unwrap_or(UNREACHED));
                }
            }
        }
        sizes.push(size);
    }
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

// ---------------------------------------------------------------------------
// A route, derived rather than authored
// ---------------------------------------------------------------------------

/// Where the milestone's named route begins.
///
/// **Old** the M6 arena clearing, chosen by the client. **New** the world's own
/// discovery overlook, [`LandmarkPlan::overlook`], which resolves to the same
/// column. **Why**: M8. A world that composes landmarks around a viewpoint has
/// to own that viewpoint — the procedural world has no notion of a player
/// spawn, but it does have a place its composition is built to be seen from,
/// and the session begins there because of that and not because M6 happened
/// to leave a clearing. `the_route_starts_at_the_world_overlook` asserts the
/// two agree, so if the composition ever moves the overlook, this constant is
/// re-derived rather than quietly wrong.
pub const ROUTE_START_X: i64 = -69;
/// See [`ROUTE_START_X`].
pub const ROUTE_START_Z: i64 = 49;

/// What the adversary's ground has to satisfy, so a fight is fought on ground
/// a viewer can see rather than inside a shrub.
///
/// The radii are M6's, for M6's reason: the client's arena predicate found that
/// the golden region offers no column clear of vegetation out to eleven units,
/// so seven is what the world actually has.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "evidence machinery; the binary reads ADVERSARY_COLUMN instead"
    )
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacementRules {
    /// The ground must be exactly level out to here, in columns.
    pub level_radius: i64,
    /// No vegetation may stand within this radius, in columns.
    pub clear_radius: i64,
    /// How far above the floor to look for vegetation, in voxels.
    pub clear_height: i64,
    /// The walk has to be a walk: at least this many steps from the start.
    pub min_steps: u32,
    /// And about this long, which is a preference rather than a rule.
    pub target_steps: u32,
    /// The landmark the fight belongs to, if the composition gives it one.
    ///
    /// With a host, the candidates are the columns around that landmark and
    /// the nearest suitable one wins; `target_steps` stops mattering, because
    /// the length of the walk is then the composition's business and not a
    /// number chosen here.
    pub host: Option<(i64, i64)>,
    /// How far from the host a body may stand, in columns.
    pub host_radius: i64,
}

impl Default for PlacementRules {
    fn default() -> Self {
        Self {
            level_radius: 7,
            clear_radius: 7,
            clear_height: 16,
            min_steps: 90,
            target_steps: 150,
            host: None,
            host_radius: 0,
        }
    }
}

impl PlacementRules {
    /// The rules for a fight hosted by a landmark.
    ///
    /// M8's answer to "why is the adversary *there*": it is there because the
    /// landmark is, and the landmark is where the world's composition put it.
    /// The walk is shorter than M7's because the first-choice landmark is
    /// closer than the far terrace was; `min_steps` still insists it is a
    /// walk and not a step out of the clearing.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "evidence machinery; the binary reads ADVERSARY_COLUMN"
        )
    )]
    #[must_use]
    pub fn hosted_by(host: (i64, i64)) -> Self {
        Self {
            min_steps: 40,
            host: Some(host),
            host_radius: 24,
            ..Self::default()
        }
    }
}

/// The landmark the adversary is posted at.
///
/// One of the two first choices, and specifically the one that does **not**
/// reveal the third: walking to one first choice shows the player a landmark
/// they could not see from the overlook, and walking to the other puts them
/// in a fight. Two directions with two different consequences is the point of
/// offering two.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "evidence machinery; the binary reads ADVERSARY_COLUMN"
    )
)]
#[must_use]
pub fn fight_host(plan: &LandmarkPlan) -> Option<&veldwake_procedural::LandmarkInstance> {
    let revealer = plan
        .instances()
        .iter()
        .find_map(|instance| instance.revealed_from);
    plan.instances().iter().find(|instance| {
        instance.role == veldwake_procedural::LandmarkRole::FirstChoice
            && Some(instance.index) != revealer
    })
}

/// Where the adversary stands in the golden region.
///
/// **Derived, then locked.** [`place_adversary`] chooses this column from the
/// region by the rules in [`PlacementRules`], and
/// `the_derived_placement_is_the_locked_one` re-derives it from the generator
/// and asserts the equality. The constant exists because that derivation costs
/// about fourteen seconds — it examines the vegetation grammar over a thousand
/// candidate clearings — and every other test wants the answer rather than the
/// search. It is a cache of a derivation, never a hand-picked pair.
///
/// **Old** none, **new** `(14, 191)`, **why**: first lock, M7. The column is
/// `162` steps from the route start, level for seven columns, clear of
/// vegetation for seven, dry, and reachable in both directions.
///
/// **Old** `(14, 191)`, **new** `(-135, 62)`, **why**: M8. The fight is no
/// longer somewhere the region merely allows one; it is at a landmark. The
/// column is the nearest suitable ground to the first-choice landmark that
/// does not reveal the third, five columns west of the spire's crown, `69`
/// steps from the overlook, and it satisfies the same four predicates M7's
/// did. What changed is the reason, and the reason is the milestone.
pub const ADVERSARY_COLUMN: (i64, i64) = (-135, 62);

/// Where the adversary stands, and the evidence that it may.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "evidence machinery; the binary reads ADVERSARY_COLUMN instead"
    )
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Placement {
    pub column: (i64, i64),
    /// Step distance from the route start.
    pub steps: u32,
    /// How many columns satisfied the cheap filters before this one was chosen.
    pub level_candidates: usize,
    /// How many of those were examined for vegetation.
    pub examined: usize,
}

/// Chooses where the adversary stands, from the region rather than by hand.
///
/// Deterministic: candidates are ordered by how close their step distance is to
/// the target, then by `(z, x)`, which is a total order, and the first that
/// satisfies every rule wins. Nothing here is a spawn system — one adversary,
/// one placement, no table and no respawn.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the placement search is run deliberately by its own test; the binary reads the locked result"
    )
)]
#[must_use]
pub fn place_adversary(
    generator: &TerrainGenerator,
    grid: &SurfaceGrid,
    report: &ReachabilityReport,
    rules: &PlacementRules,
) -> Option<Placement> {
    // Cheap filters first, over the whole grid: reachable both ways, dry, far
    // enough, and standing on level ground. Only what survives all of that is
    // worth asking the vegetation grammar about.
    let mut candidates: Vec<(u32, i64, i64, u32)> = Vec::new();
    for index in 0..grid.len() {
        let Some((x, z)) = grid.column_of(index) else {
            continue;
        };
        let Some(steps) = report.distance_to(grid, x, z) else {
            continue;
        };
        if steps < rules.min_steps {
            continue;
        }
        if !grid.standable(x, z) {
            continue;
        }
        if !is_level(grid, x, z, rules.level_radius) {
            continue;
        }
        let rank = match rules.host {
            Some((host_x, host_z)) => {
                let (dx, dz) = (x - host_x, z - host_z);
                let reach = dx * dx + dz * dz;
                if reach > rules.host_radius * rules.host_radius {
                    continue;
                }
                u32::try_from(reach).unwrap_or(u32::MAX)
            }
            None => steps.abs_diff(rules.target_steps),
        };
        candidates.push((rank, z, x, steps));
    }
    let level_candidates = candidates.len();
    candidates.sort_unstable();

    let vegetation = generator.vegetation();
    for (examined, (_, z, x, steps)) in candidates.into_iter().enumerate() {
        let Some(floor) = grid.support_at(x, z) else {
            continue;
        };
        if is_clear_of_vegetation(
            &vegetation,
            x,
            z,
            i64::from(floor),
            rules.clear_radius,
            rules.clear_height,
        ) {
            return Some(Placement {
                column: (x, z),
                steps,
                level_candidates,
                examined: examined.saturating_add(1),
            });
        }
    }
    None
}

/// Whether the ground is exactly level over a disc of columns.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "a placement predicate; only the search and its tests ask it"
    )
)]
#[must_use]
pub fn is_level(grid: &SurfaceGrid, x: i64, z: i64, radius: i64) -> bool {
    let Some(height) = grid.support_at(x, z) else {
        return false;
    };
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            let (sx, sz) = (x.saturating_add(dx), z.saturating_add(dz));
            if grid.support_at(sx, sz) != Some(height) || grid.has_water(sx, sz) {
                return false;
            }
        }
    }
    true
}

/// Whether no plant stands in a column disc, up to a height above the floor.
///
/// There is no vegetation collision, so a fight that could reach a shrub would
/// be a fight inside one.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "a placement predicate; only the search and its tests ask it"
    )
)]
#[must_use]
pub fn is_clear_of_vegetation(
    vegetation: &veldwake_procedural::WorldVegetation<'_>,
    x: i64,
    z: i64,
    floor: i64,
    radius: i64,
    height: i64,
) -> bool {
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            for y in floor..floor.saturating_add(height) {
                if vegetation.occupied(x.saturating_add(dx), y, z.saturating_add(dz)) {
                    return false;
                }
            }
        }
    }
    true
}

/// A named place along the route, with the claim its name makes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Checkpoint {
    pub name: &'static str,
    pub column: (i64, i64),
    /// Step distance from the start.
    pub steps: u32,
    /// What a capture taken here is meant to show.
    pub intent: &'static str,
}

/// The named route: a path the rules themselves say is walkable.
///
/// It is an **evidence fixture**, not a navigation system. Gameplay has no
/// pathfinding: a player walks with the keyboard, and this exists so that the
/// walk can be proved possible, measured, captured at named places, and
/// reproduced exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub start: (i64, i64),
    pub goal: (i64, i64),
    /// Every column the derived path passes through.
    pub columns: Vec<(i64, i64)>,
    /// The path with collinear runs collapsed: the corners of the walk.
    pub waypoints: Vec<Vec2>,
    /// Total length of the waypoint polyline, in world units.
    pub length: f32,
    pub checkpoints: Vec<Checkpoint>,
}

impl Route {
    /// How long the walk takes at a speed, in seconds.
    #[must_use]
    pub fn duration_at(&self, speed: f32) -> f32 {
        if speed <= 0.0 {
            return f32::INFINITY;
        }
        self.length / speed
    }

    /// How many chunk boundaries the path crosses, on each axis.
    #[must_use]
    pub fn chunk_crossings(&self) -> (usize, usize) {
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let mut x_crossings = 0;
        let mut z_crossings = 0;
        for pair in self.columns.windows(2) {
            let [(ax, az), (bx, bz)] = [pair[0], pair[1]];
            if ax.div_euclid(edge) != bx.div_euclid(edge) {
                x_crossings += 1;
            }
            if az.div_euclid(edge) != bz.div_euclid(edge) {
                z_crossings += 1;
            }
        }
        (x_crossings, z_crossings)
    }
}

/// Derives the named route from a finished audit.
///
/// The path is the audit's own breadth-first tree, so it is the shortest walk
/// the rules allow and nothing about it was chosen by hand.
#[must_use]
pub fn derive_route(
    grid: &SurfaceGrid,
    report: &ReachabilityReport,
    goal: (i64, i64),
) -> Option<Route> {
    let columns = report.path_to(grid, goal.0, goal.1)?;
    if columns.len() < 2 {
        return None;
    }
    let waypoints = collapse_collinear(&columns);
    let mut length = 0.0_f32;
    for pair in waypoints.windows(2) {
        length += (pair[1] - pair[0]).length();
    }
    let checkpoints = name_checkpoints(grid, report, &columns);
    Some(Route {
        start: columns.first().copied()?,
        goal,
        columns,
        waypoints,
        length,
        checkpoints,
    })
}

/// Keeps only the corners of a column path.
fn collapse_collinear(columns: &[(i64, i64)]) -> Vec<Vec2> {
    let mut waypoints = Vec::new();
    let mut previous_step: Option<(i64, i64)> = None;
    for (index, &(x, z)) in columns.iter().enumerate() {
        let step = columns.get(index + 1).map(|&(nx, nz)| (nx - x, nz - z));
        let corner = index == 0 || step.is_none() || step != previous_step;
        if corner {
            waypoints.push(column_centre(x, z));
        }
        if step.is_some() {
            previous_step = step;
        }
    }
    waypoints
}

/// Names places along the route by what is actually there.
///
/// Every name is a claim, and `every_checkpoint_satisfies_the_claim_its_name_makes`
/// is where each claim is checked. A camera pose is a fixture and so is this.
fn name_checkpoints(
    grid: &SurfaceGrid,
    report: &ReachabilityReport,
    columns: &[(i64, i64)],
) -> Vec<Checkpoint> {
    let mut checkpoints = Vec::new();
    let steps_of = |x: i64, z: i64| report.distance_to(grid, x, z).unwrap_or_default();

    let Some(&(sx, sz)) = columns.first() else {
        return checkpoints;
    };
    checkpoints.push(Checkpoint {
        name: "route-start",
        column: (sx, sz),
        steps: steps_of(sx, sz),
        intent: "where the session begins: level, dry, clear ground under the body",
    });

    let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
    if let Some(&(x, z)) = columns.iter().find(|&&(x, z)| {
        x.div_euclid(edge) != sx.div_euclid(edge) || z.div_euclid(edge) != sz.div_euclid(edge)
    }) {
        checkpoints.push(Checkpoint {
            name: "first-chunk-crossing",
            column: (x, z),
            steps: steps_of(x, z),
            intent: "the first chunk the walk leaves: streaming under a moving body",
        });
    }

    // Closest dry approach to water anywhere along the walk.
    let mut closest: Option<(i64, (i64, i64))> = None;
    for &(x, z) in columns {
        let mut best = i64::MAX;
        for dz in -6_i64..=6 {
            for dx in -6_i64..=6 {
                if grid.has_water(x.saturating_add(dx), z.saturating_add(dz)) {
                    best = best.min(dx * dx + dz * dz);
                }
            }
        }
        if best < i64::MAX && closest.is_none_or(|(current, _)| best < current) {
            closest = Some((best, (x, z)));
        }
    }
    if let Some((_, (x, z))) = closest {
        checkpoints.push(Checkpoint {
            name: "water-edge",
            column: (x, z),
            steps: steps_of(x, z),
            intent: "the closest the walk comes to water it may not enter",
        });
    }

    // The steepest legal leg, by support height change per column step.
    let mut steepest: Option<(i32, (i64, i64))> = None;
    for pair in columns.windows(2) {
        let [(ax, az), (bx, bz)] = [pair[0], pair[1]];
        let (Some(a), Some(b)) = (grid.support_at(ax, az), grid.support_at(bx, bz)) else {
            continue;
        };
        let rise = (b - a).abs();
        if steepest.is_none_or(|(best, _)| rise > best) {
            steepest = Some((rise, (bx, bz)));
        }
    }
    if let Some((rise, (x, z))) = steepest
        && rise > 0
    {
        checkpoints.push(Checkpoint {
            name: "steepest-leg",
            column: (x, z),
            steps: steps_of(x, z),
            intent: "the largest height change the walk actually takes",
        });
    }

    if let Some(&(x, z)) = columns.last() {
        checkpoints.push(Checkpoint {
            name: "route-goal",
            column: (x, z),
            steps: steps_of(x, z),
            intent: "where the adversary stands: the fight at the end of the walk",
        });
    }
    checkpoints
}

/// The encounter a traversal session is played in.
///
/// The same two combatants, the same weapon and the same tuning M6 fights with.
/// What differs is placement and bounds: the player starts at the route start,
/// the adversary at the derived column a route away, there is no arena disc, and
/// beating the adversary leaves it where it fell instead of starting the round
/// again.
///
/// # Panics
///
/// Never in practice. `ArenaSpec` is the only fallible part of a setup and this
/// one has none; the starts are locked columns of the golden region.
#[must_use]
pub fn traversal_setup() -> veldwake_combat::EncounterSetup {
    let mut setup = veldwake_combat::fixture::golden_setup();
    setup.starts = [
        column_centre(ROUTE_START_X, ROUTE_START_Z),
        column_centre(ADVERSARY_COLUMN.0, ADVERSARY_COLUMN.1),
    ];
    // The region and the water veto are the bounds. A disc around one of the
    // two bodies would be a fence across a valley.
    setup.arena = None;
    // The session continues past the fight, which is the whole milestone.
    setup.player_victory = veldwake_combat::PlayerVictoryPolicy::Remain;
    // M9: the gate holds a weapon the player can take. Where the site is, is
    // the client's answer through `reward::site`; what the exchange *is* is
    // combat's, and this is where the two are joined.
    setup.reward = Some(veldwake_combat::fixture::reward_setup());
    setup
}

// ---------------------------------------------------------------------------
// Route identity
// ---------------------------------------------------------------------------

/// Locked signature of the golden route.
///
/// **Old** none, **new** `0x08c1_0aea_5280_b90f`, **why**: first lock, M7. The
/// route runs from the M6 arena clearing at `(-69, 49)` to the derived
/// adversary column at `(14, 191)`: 163 columns, 40 waypoints, `217.09` world
/// units, crossing three chunk boundaries on `x` and four on `z`.
///
/// **Old** `0x08c1_0aea_5280_b90f`, **new** `0xa06c_9d72_a882_4848`, **why**: M8,
/// and three things moved at once. The world identity changed, because
/// landmarks are part of what it generates. [`TRAVERSAL_RULE_VERSION`] went to
/// `2`, because a landmark is a new reason to refuse a destination. And the
/// walk itself moved, because [`ADVERSARY_COLUMN`] moved to the landmark that
/// hosts the fight: `70` columns, `5` waypoints, `97.17` units and `28.6`
/// seconds at `3.40` u/s, against M7's `163`, `40`, `217.09` and `63.9`.
///
/// Re-locking requires an OLD/NEW/WHY paragraph here naming the semantic change
/// that moved it, exactly as M5 and M6 require of theirs. The signature covers
/// the world identity, the movement spec, [`TRAVERSAL_RULE_VERSION`], the two
/// endpoints, every waypoint and every checkpoint — so a changed `max_step_up`
/// moves it even if the path happens not to change.
pub const GOLDEN_ROUTE_SIGNATURE: u64 = 0xa06c_9d72_a882_4848;

/// Hash of everything that decides what the route is.
///
/// The movement spec and [`TRAVERSAL_RULE_VERSION`] are in it on purpose: a
/// change to `max_step_up` or to what water means can leave every waypoint
/// alone and still mean the route was derived under different rules. Nothing
/// about rendering, camera or timing is in it, because none of those decide a
/// path.
#[must_use]
pub fn route_signature(
    generator: &TerrainGenerator,
    movement: &MovementSpec,
    route: &Route,
) -> u64 {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"veldwake.m7.route");
    bytes.extend_from_slice(&generator.fingerprint().to_le_bytes());
    bytes.extend_from_slice(&TRAVERSAL_RULE_VERSION.to_le_bytes());
    for value in [
        movement.speed(),
        movement.turn_rate(),
        movement.max_step_up(),
        movement.max_drop(),
        movement.recovery_speed_scale(),
    ] {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    for (x, z) in [route.start, route.goal] {
        bytes.extend_from_slice(&x.to_le_bytes());
        bytes.extend_from_slice(&z.to_le_bytes());
    }
    bytes.extend_from_slice(
        &u64::try_from(route.columns.len())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    for waypoint in &route.waypoints {
        bytes.extend_from_slice(&waypoint.x.to_bits().to_le_bytes());
        bytes.extend_from_slice(&waypoint.y.to_bits().to_le_bytes());
    }
    for checkpoint in &route.checkpoints {
        bytes.extend_from_slice(checkpoint.name.as_bytes());
        bytes.extend_from_slice(&checkpoint.column.0.to_le_bytes());
        bytes.extend_from_slice(&checkpoint.column.1.to_le_bytes());
    }
    fnv1a64(&bytes)
}

/// Sixty-four-bit FNV-1a.
///
/// The fourth copy of this function in the workspace, and deliberately so.
/// `veldwake-procedural`, `veldwake-character` and `veldwake-combat` each carry
/// their own for the reason `DETERMINISM.md` records: making a crate's
/// internals public to serve a consumer buys code reuse at the price of the
/// dependency direction in `ARCHITECTURE.md`. The route is client evidence, the
/// client is nobody's dependency, and the function is pinned against the
/// published vectors by `fnv1a_matches_the_published_vectors`.
#[must_use]
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{
        ADVERSARY_COLUMN, BODY_KEEP_OUT_HEIGHT, BODY_KEEP_OUT_RADIUS, GOLDEN_ROUTE_SIGNATURE,
        PlacementRules, ROUTE_START_X, ROUTE_START_Z, Route, SurfaceGrid, audit, column_centre,
        derive_route, fnv1a64, is_clear_of_vegetation, is_level, place_adversary, route_signature,
    };
    use crate::character::TerrainGround;
    use crate::traversal::TerrainWalkability;
    use glam::Vec2;
    use std::collections::{HashMap, HashSet};
    use std::time::Instant;
    use veldwake_character::GroundSampler;
    use veldwake_combat::{
        Encounter, Intent, MovementSpec, PlayerVictoryPolicy, Side, TraversalLegality,
        WorldContact, fixture,
    };
    use veldwake_procedural::{TerrainGenerator, material::TerrainMaterial};
    use veldwake_voxel::{CHUNK_EDGE, ChunkCoord};

    fn generator() -> TerrainGenerator {
        TerrainGenerator::golden()
    }

    fn movement() -> MovementSpec {
        match fixture::tuning().compile() {
            Ok(tuning) => *tuning.movement(),
            Err(error) => panic!("the fixture tuning compiles: {error}"),
        }
    }

    /// Chunks the water and agreement tests walk exhaustively.
    ///
    /// The river runs along the valley axis, so these span dry meadow, the
    /// channel, both banks, negative coordinates and a chunk seam across the
    /// water.
    const WATER_CHUNKS: [(i32, i32); 6] = [(0, 0), (0, 1), (-1, 0), (3, 0), (-5, -1), (2, 2)];

    /// Chunks the ground-agreement tests walk exhaustively.
    ///
    /// Deliberately not [`WATER_CHUNKS`], so the ground oracle is not asking
    /// the same question about the same terrain twice: meadow, the climb
    /// towards the highland, and chunks on either side of the origin.
    const GROUND_CHUNKS: [(i32, i32); 5] = [(0, 0), (-3, 1), (2, -4), (3, 4), (-6, -2)];

    #[test]
    fn fnv1a_matches_the_published_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn the_cached_grid_agrees_with_the_runtime_adapters() {
        // Without this the grid would be a second, silently drifting copy of
        // the world, and every number the audit produced would be about a
        // region the game does not have.
        let generator = generator();
        let grid = SurfaceGrid::sample(&generator);
        let ground = TerrainGround::new(&generator);
        let veto = TerrainWalkability::new(&generator);
        let cached_ground = grid.ground();
        let cached_veto = grid.legality();

        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let mut checked = 0_u64;
        for (chunk_x, chunk_z) in WATER_CHUNKS {
            for local_z in 0..edge {
                for local_x in 0..edge {
                    let x = i64::from(chunk_x) * edge + local_x;
                    let z = i64::from(chunk_z) * edge + local_z;
                    let centre = column_centre(x, z);
                    let (cx, cz) = (f64::from(centre.x), f64::from(centre.y));
                    assert_eq!(
                        cached_ground.surface(cx, cz),
                        ground.surface(cx, cz),
                        "the cached grid disagrees about the ground at ({x}, {z})"
                    );
                    assert_eq!(
                        cached_veto.walkable(cx, cz),
                        veto.walkable(cx, cz),
                        "the cached grid disagrees about traversal at ({x}, {z})"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 6_000, "only {checked} columns were compared");
    }

    /// The topmost generated ground voxel of every column, for the chunks the
    /// ground-agreement tests walk.
    ///
    /// Built from the voxels themselves rather than from any adapter this
    /// module already trusts, which is the whole point: it is an oracle, not a
    /// second opinion. Water is not ground and neither is a tree - the M5
    /// contract is that `TerrainGround` reads the terrain field and knows
    /// nothing about vegetation - so only the terrain materials count.
    fn generated_ground_tops(generator: &TerrainGenerator, chunk_x: i32, chunk_z: i32) -> Vec<i64> {
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let mut top = vec![i64::MIN; CHUNK_EDGE * CHUNK_EDGE];
        // The region is three chunks tall, so a column's ground can be in any
        // of them.
        for chunk_y in 0..3_i32 {
            let Some(chunk) = generator.generate(ChunkCoord::new(chunk_x, chunk_y, chunk_z)) else {
                continue;
            };
            for local_z in 0..CHUNK_EDGE {
                for local_x in 0..CHUNK_EDGE {
                    for local_y in 0..CHUNK_EDGE {
                        let Ok(local) = veldwake_voxel::LocalCoord::new(local_x, local_y, local_z)
                        else {
                            continue;
                        };
                        let is_ground = matches!(
                            TerrainMaterial::from_voxel_id(chunk.read_local(local)),
                            Some(
                                TerrainMaterial::MeadowGrass
                                    | TerrainMaterial::HighlandGrass
                                    | TerrainMaterial::Soil
                                    | TerrainMaterial::Rock
                                    | TerrainMaterial::DeepRock
                                    | TerrainMaterial::Sediment
                            )
                        );
                        if !is_ground {
                            continue;
                        }
                        let world_y =
                            i64::from(chunk_y) * edge + i64::try_from(local_y).unwrap_or(i64::MAX);
                        let slot = local_z * CHUNK_EDGE + local_x;
                        if world_y > top[slot] {
                            top[slot] = world_y;
                        }
                    }
                }
            }
        }
        top
    }

    #[test]
    fn the_ground_query_matches_the_generated_voxels_at_every_column_centre() {
        // The counterpart to `traversal_water_agrees_with_the_voxels_the_generator_writes`,
        // for the other half of a column: its floor.
        //
        // Everything in this module reasons about columns. `SurfaceGrid` stores
        // one height per column, the audit walks column centres, and the route
        // is a list of columns. `TerrainGround`, though, answers a continuous
        // question: it samples the terrain field wherever it is asked. The two
        // agree at a column's centre, exactly, and this is the oracle that says
        // so from the drawn cells rather than from an adapter.
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let mut checked = 0_u64;
        let mut differing = Vec::new();

        for (chunk_x, chunk_z) in GROUND_CHUNKS {
            let top = generated_ground_tops(&generator, chunk_x, chunk_z);
            for local_z in 0..CHUNK_EDGE {
                for local_x in 0..CHUNK_EDGE {
                    let slot = local_z * CHUNK_EDGE + local_x;
                    if top[slot] == i64::MIN {
                        continue;
                    }
                    #[expect(clippy::cast_precision_loss, reason = "small world coordinates")]
                    let face = (top[slot] + 1) as f64;
                    let x = i64::from(chunk_x) * edge + i64::try_from(local_x).unwrap_or(i64::MAX);
                    let z = i64::from(chunk_z) * edge + i64::try_from(local_z).unwrap_or(i64::MAX);
                    let centre = column_centre(x, z);
                    let Some(height) = ground.surface(f64::from(centre.x), f64::from(centre.y))
                    else {
                        continue;
                    };
                    checked += 1;
                    if (height - face).abs() > 1.0e-9 && differing.len() < 8 {
                        differing.push((x, z, face, height));
                    }
                }
            }
        }
        assert!(
            checked > 4_000,
            "only {checked} column centres were compared"
        );
        assert!(
            differing.is_empty(),
            "the ground query disagrees with the drawn voxels at a column centre: {differing:?}"
        );
    }

    #[test]
    fn away_from_a_column_centre_the_ground_query_may_differ_but_only_within_two_voxels() {
        // The honest limit of every column-shaped claim in this module.
        //
        // The generator decides a whole column from its centre; `TerrainGround`
        // samples the field at whatever point it is handed. Away from the
        // centre they genuinely disagree - on a slope the continuous field has
        // already moved on while the column has not - and a body walks through
        // those points, not through centres. That is precisely why the
        // reachability audit is an upper bound on a column graph and why the
        // route is then re-proved against the runtime adapters by
        // `the_runtime_accepts_every_step_of_the_route_it_walks_continuously`.
        //
        // Two things are asserted, and the first matters as much as the second:
        // the disagreement is real, so nobody may simplify it away; and it is
        // bounded, so a column height is never further from what a body
        // standing anywhere in that column finds than the step-up and drop
        // rules already tolerate.
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let mut checked = 0_u64;
        let mut differing = 0_u64;
        let mut worst = 0.0_f64;
        let mut worst_at = (0_i64, 0_i64);

        for (chunk_x, chunk_z) in GROUND_CHUNKS {
            let top = generated_ground_tops(&generator, chunk_x, chunk_z);
            for local_z in 0..CHUNK_EDGE {
                for local_x in 0..CHUNK_EDGE {
                    let slot = local_z * CHUNK_EDGE + local_x;
                    if top[slot] == i64::MIN {
                        continue;
                    }
                    #[expect(clippy::cast_precision_loss, reason = "small world coordinates")]
                    let face = (top[slot] + 1) as f64;
                    let x = i64::from(chunk_x) * edge + i64::try_from(local_x).unwrap_or(i64::MAX);
                    let z = i64::from(chunk_z) * edge + i64::try_from(local_z).unwrap_or(i64::MAX);
                    #[expect(clippy::cast_precision_loss, reason = "small world coordinates")]
                    let (fx, fz) = (x as f64, z as f64);
                    for (offset_x, offset_z) in
                        [(0.05, 0.05), (0.95, 0.05), (0.05, 0.95), (0.95, 0.95)]
                    {
                        let Some(height) = ground.surface(fx + offset_x, fz + offset_z) else {
                            continue;
                        };
                        checked += 1;
                        let delta = (height - face).abs();
                        if delta > 1.0e-9 {
                            differing += 1;
                            if delta > worst {
                                worst = delta;
                                worst_at = (x, z);
                            }
                        }
                    }
                }
            }
        }
        assert!(
            checked > 16_000,
            "only {checked} offset points were compared"
        );
        assert!(
            differing > 0,
            "the column and the field never disagreed, which would make the audit exact; \
             if that is now true it is a change worth recording, not a silent one"
        );
        assert!(
            worst <= 2.0,
            "the ground query is {worst} voxels from its column at {worst_at:?}, \
             which is further than the step-up and drop rules tolerate"
        );
    }

    #[test]
    fn the_water_veto_disagrees_inside_a_column_only_along_the_waterline() {
        // The water half of the same honest limit as
        // `away_from_a_column_centre_the_ground_query_may_differ_but_only_within_two_voxels`.
        //
        // `TerrainWalkability` samples the field continuously, exactly as
        // `TerrainGround` does; it is `SurfaceGrid`, the audit and the route
        // that are column-shaped. So a column is not uniformly wet or
        // uniformly dry: near the drawn waterline the quantised predicate
        // flips inside a single column, and a body crossing that column can be
        // refused at a point the audit accepted at the centre.
        //
        // What this pins is where that is allowed to happen. Every column that
        // disagrees with itself must be a shore column - one with a neighbour
        // whose verdict differs - so the fuzziness is a one-column band along
        // the waterline and never a hole in the middle of the meadow.
        let generator = generator();
        let veto = TerrainWalkability::new(&generator);
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let centre_verdict = |x: i64, z: i64| {
            let centre = column_centre(x, z);
            veto.walkable(f64::from(centre.x), f64::from(centre.y))
        };

        let mut columns = 0_u64;
        let mut inconsistent = 0_u64;
        let mut inland = Vec::new();
        for (chunk_x, chunk_z) in WATER_CHUNKS {
            for local_z in 0..edge {
                for local_x in 0..edge {
                    let x = i64::from(chunk_x) * edge + local_x;
                    let z = i64::from(chunk_z) * edge + local_z;
                    let expected = centre_verdict(x, z);
                    columns += 1;
                    #[expect(clippy::cast_precision_loss, reason = "small world coordinates")]
                    let (fx, fz) = (x as f64, z as f64);
                    let mut differs = false;
                    for offset_x in [0.01_f64, 0.25, 0.5, 0.75, 0.99] {
                        for offset_z in [0.01_f64, 0.25, 0.5, 0.75, 0.99] {
                            if veto.walkable(fx + offset_x, fz + offset_z) != expected {
                                differs = true;
                            }
                        }
                    }
                    if !differs {
                        continue;
                    }
                    inconsistent += 1;
                    let on_the_shore = [
                        (1_i64, 0_i64),
                        (-1, 0),
                        (0, 1),
                        (0, -1),
                        (1, 1),
                        (1, -1),
                        (-1, 1),
                        (-1, -1),
                    ]
                    .into_iter()
                    .any(|(step_x, step_z)| centre_verdict(x + step_x, z + step_z) != expected);
                    if !on_the_shore && inland.len() < 8 {
                        inland.push((x, z, expected));
                    }
                }
            }
        }
        assert!(columns > 5_000, "only {columns} columns were compared");
        assert!(
            inconsistent > 0,
            "no column disagreed with itself, which would mean the veto had become \
             column-quantised; that is a change worth recording, not a silent one"
        );
        assert!(
            inland.is_empty(),
            "a column disagrees with itself away from any waterline: {inland:?}"
        );
    }

    #[test]
    fn traversal_water_agrees_with_the_voxels_the_generator_writes() {
        // INVARIANTS.md TRAVERSE-001, asserted against generated chunks rather
        // than against the predicate that decides it.
        let generator = generator();
        let veto = TerrainWalkability::new(&generator);
        let mut wet_columns = 0_u64;
        let mut dry_columns = 0_u64;
        for (chunk_x, chunk_z) in WATER_CHUNKS {
            // The region is three chunks tall, so a column's water can be in
            // any of them.
            let mut water_in_column = vec![false; CHUNK_EDGE * CHUNK_EDGE];
            for chunk_y in 0..3_i32 {
                let Some(chunk) = generator.generate(ChunkCoord::new(chunk_x, chunk_y, chunk_z))
                else {
                    continue;
                };
                for local_z in 0..CHUNK_EDGE {
                    for local_x in 0..CHUNK_EDGE {
                        for local_y in 0..CHUNK_EDGE {
                            let Ok(local) =
                                veldwake_voxel::LocalCoord::new(local_x, local_y, local_z)
                            else {
                                continue;
                            };
                            if chunk.read_local(local) == TerrainMaterial::Water.voxel_id() {
                                water_in_column[local_z * CHUNK_EDGE + local_x] = true;
                            }
                        }
                    }
                }
            }
            let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
            for local_z in 0..CHUNK_EDGE {
                for local_x in 0..CHUNK_EDGE {
                    let x = i64::from(chunk_x) * edge + local_x as i64;
                    let z = i64::from(chunk_z) * edge + local_z as i64;
                    let centre = column_centre(x, z);
                    let walkable = veto.walkable(f64::from(centre.x), f64::from(centre.y));
                    let wet = water_in_column[local_z * CHUNK_EDGE + local_x];
                    assert_eq!(
                        walkable, !wet,
                        "traversal and the drawn voxels disagree at ({x}, {z})"
                    );
                    if wet {
                        wet_columns += 1;
                    } else {
                        dry_columns += 1;
                    }
                }
            }
        }
        assert!(wet_columns > 0, "no water was tested");
        assert!(dry_columns > 0, "no dry ground was tested");
    }

    #[test]
    fn the_continuous_and_the_voxel_water_predicates_genuinely_disagree() {
        // If they agreed everywhere the distinction would be pedantry. They do
        // not: `is_submerged` is a relation between two continuous fields and
        // `has_water_voxel` is a statement about the blocks a viewer sees.
        let generator = generator();
        let field = generator.field();
        let veto = TerrainWalkability::new(&generator);
        let mut disagreements = 0_u64;
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        for (chunk_x, chunk_z) in WATER_CHUNKS {
            for local_z in 0..edge {
                for local_x in 0..edge {
                    let x = i64::from(chunk_x) * edge + local_x;
                    let z = i64::from(chunk_z) * edge + local_z;
                    let centre = column_centre(x, z);
                    let (cx, cz) = (f64::from(centre.x), f64::from(centre.y));
                    let sample = field.sample(cx, cz);
                    if sample.is_submerged() != sample.has_water_voxel() {
                        disagreements += 1;
                        // And traversal follows the voxels, never the relation.
                        assert_eq!(
                            veto.walkable(cx, cz),
                            !sample.has_water_voxel(),
                            "traversal followed the continuous relation at ({x}, {z})"
                        );
                    }
                }
            }
        }
        assert!(
            disagreements > 0,
            "the two water predicates never disagreed, so the distinction is untested"
        );
    }

    #[test]
    fn the_grid_indexes_exactly_the_region_its_bounds_describe() {
        // The region's extent is interpreted twice: `RegionBounds::of` turns it
        // into a continuous half-open rectangle for the veto, and
        // `SurfaceGrid::sample` turns the same extent into integer column
        // indices. They agree by construction and nothing said so, which is the
        // shape a second, silently drifting notion of "the region" would take.
        //
        // Asserted at the corners of both, in both directions: the first and
        // last column the grid will index must be inside the bounds, and the
        // column just outside the grid must be outside them.
        let generator = generator();
        let grid = SurfaceGrid::sample(&generator);
        // The grid keeps its own copy of the bounds; a test in this module can
        // read it directly, which is the point - no accessor exists and none
        // should, because nothing outside needs it.
        let bounds = grid.bounds;

        let Some((min_x, min_z)) = grid.column_of(0) else {
            panic!("a region with no columns is not a region");
        };
        let Some((max_x, max_z)) = grid.column_of(grid.len() - 1) else {
            panic!("the grid cannot address its own last column");
        };
        assert_eq!(
            (
                grid.columns_x(),
                grid.columns_z(),
                grid.columns_x() * grid.columns_z()
            ),
            (
                usize::try_from(max_x - min_x + 1).unwrap_or_default(),
                usize::try_from(max_z - min_z + 1).unwrap_or_default(),
                grid.len()
            ),
            "the grid's shape and its addressable columns disagree"
        );

        let inside = |x: i64, z: i64| {
            let centre = column_centre(x, z);
            bounds.contains(f64::from(centre.x), f64::from(centre.y))
        };
        assert!(
            inside(min_x, min_z),
            "the grid's first column is outside the bounds"
        );
        assert!(
            inside(max_x, max_z),
            "the grid's last column is outside the bounds"
        );
        assert!(
            !inside(min_x - 1, min_z),
            "the bounds reach a column the grid does not"
        );
        assert!(
            !inside(max_x + 1, max_z),
            "the bounds reach a column the grid does not"
        );
        assert!(
            !inside(min_x, min_z - 1),
            "the bounds reach a column the grid does not"
        );
        assert!(
            !inside(max_x, max_z + 1),
            "the bounds reach a column the grid does not"
        );
        assert!(grid.index_of(min_x - 1, min_z).is_none());
        assert!(grid.index_of(max_x + 1, max_z).is_none());
        assert!(grid.index_of(min_x, min_z - 1).is_none());
        assert!(grid.index_of(max_x, max_z + 1).is_none());
    }

    #[test]
    fn traversal_stops_at_the_region_edge_as_a_half_open_rectangle() {
        let generator = generator();
        let veto = TerrainWalkability::new(&generator);
        let bounds = veto.bounds();
        // Just inside on every side, including the fractional half of the last
        // column, and just outside.
        assert!(veto.walkable(bounds.min_x(), bounds.min_z()));
        assert!(veto.walkable(
            bounds.max_x_exclusive() - 0.001,
            bounds.max_z_exclusive() - 0.001
        ));
        assert!(!veto.walkable(bounds.max_x_exclusive(), 0.0));
        assert!(!veto.walkable(0.0, bounds.max_z_exclusive()));
        assert!(!veto.walkable(bounds.min_x() - 0.001, 0.0));
        assert!(!veto.walkable(0.0, bounds.min_z() - 0.001));
        assert!(!veto.walkable(f64::NAN, 0.0));
        assert!(!veto.walkable(0.0, f64::INFINITY));
    }

    #[test]
    fn a_fractional_position_is_judged_by_the_column_that_contains_it() {
        let generator = generator();
        let veto = TerrainWalkability::new(&generator);
        // Any column: every point inside it must get the same verdict, and
        // negative coordinates must floor rather than truncate.
        for (x, z) in [(-70_i64, 48_i64), (-3, -3), (0, 0), (5, -120)] {
            #[expect(clippy::cast_precision_loss, reason = "region coordinates are small")]
            let base = (x as f64, z as f64);
            let verdict = veto.walkable(base.0 + 0.5, base.1 + 0.5);
            for (dx, dz) in [(0.01, 0.01), (0.99, 0.99), (0.5, 0.01), (0.01, 0.99)] {
                assert_eq!(
                    veto.walkable(base.0 + dx, base.1 + dz),
                    verdict,
                    "the column at ({x}, {z}) answered differently at ({dx}, {dz})"
                );
            }
        }
    }

    #[test]
    fn water_blocks_in_the_channel_and_not_on_the_bank() {
        let generator = generator();
        let field = generator.field();
        let veto = TerrainWalkability::new(&generator);
        // Walk across the valley at a few places along the axis and find the
        // channel; the transition must exist and must be a real boundary.
        let mut found_transition = false;
        for x in [-200_i64, -60, 40, 180] {
            let mut previous: Option<bool> = None;
            #[expect(clippy::cast_precision_loss, reason = "region coordinates are small")]
            let fx = x as f64 + 0.5;
            for z in -200_i64..200 {
                #[expect(clippy::cast_precision_loss, reason = "region coordinates are small")]
                let fz = z as f64 + 0.5;
                let walkable = veto.walkable(fx, fz);
                if previous == Some(true) && !walkable {
                    found_transition = true;
                    assert!(
                        field.sample(fx, fz).has_water_voxel(),
                        "a non-water column blocked traversal at ({x}, {z})"
                    );
                }
                previous = Some(walkable);
            }
        }
        assert!(found_transition, "no shoreline was crossed");
    }

    // -----------------------------------------------------------------------
    // The audit, the route and the placement, all derived from the region
    // -----------------------------------------------------------------------

    struct Derived {
        generator: TerrainGenerator,
        grid: SurfaceGrid,
        report: super::ReachabilityReport,
        route: Route,
        sample_ms: u128,
        audit_ms: u128,
        route_ms: u128,
    }

    /// Samples, audits, and derives the route to the **locked** adversary
    /// column.
    ///
    /// Sampling and the audit together cost under a second on the audited host;
    /// searching the region for the adversary's clearing costs about fourteen,
    /// because it asks the vegetation grammar about a thousand candidates. The
    /// search is therefore re-run deliberately by
    /// `the_derived_placement_is_the_locked_one` rather than by every test that
    /// wants a route. That split is a cost observation, not a budget: nothing
    /// here asserts a duration.
    fn derive() -> Derived {
        let generator = generator();
        let movement = movement();
        let start = (ROUTE_START_X, ROUTE_START_Z);

        let clock = Instant::now();
        let grid = SurfaceGrid::sample(&generator);
        let sample_ms = clock.elapsed().as_millis();

        let clock = Instant::now();
        let report = audit(&grid, &movement, start);
        let audit_ms = clock.elapsed().as_millis();

        let clock = Instant::now();
        let Some(route) = derive_route(&grid, &report, ADVERSARY_COLUMN) else {
            panic!("the locked adversary column is not reachable from the route start");
        };
        let route_ms = clock.elapsed().as_millis();

        Derived {
            generator,
            grid,
            report,
            route,
            sample_ms,
            audit_ms,
            route_ms,
        }
    }

    #[test]
    fn the_route_start_is_a_place_a_body_can_stand() {
        let generator = generator();
        let grid = SurfaceGrid::sample(&generator);
        assert!(
            grid.standable(ROUTE_START_X, ROUTE_START_Z),
            "the route starts somewhere a body may not be"
        );
        assert!(
            is_level(&grid, ROUTE_START_X, ROUTE_START_Z, 7),
            "the route starts on ground that is not level"
        );
        let Some(floor) = grid.support_at(ROUTE_START_X, ROUTE_START_Z) else {
            panic!("the route start has no ground");
        };
        assert!(
            is_clear_of_vegetation(
                &generator.vegetation(),
                ROUTE_START_X,
                ROUTE_START_Z,
                i64::from(floor),
                7,
                16,
            ),
            "the route starts inside a plant"
        );
    }

    #[test]
    fn the_audit_and_everything_derived_from_it_is_reproducible() {
        let first = derive();
        let second = derive();
        assert_eq!(first.report.forward, second.report.forward);
        assert_eq!(first.report.reverse, second.report.reverse);
        assert_eq!(first.report.barriers, second.report.barriers);
        assert_eq!(first.report.component_sizes, second.report.component_sizes);
        assert_eq!(first.route, second.route);
    }

    #[test]
    fn the_audit_answers_the_questions_the_milestone_asked_it() {
        // These are the region's properties, not targets. They are asserted so
        // that a change to the world or to the movement rule which silently
        // halves what a player can reach fails a test instead of a playtest.
        let derived = derive();
        let report = &derived.report;
        assert_eq!(report.columns_total, 640_000);
        assert_eq!(report.columns_with_ground, 640_000, "the region is solid");
        assert!(
            report.columns_standable < report.columns_with_ground,
            "no column is water, so the water veto is doing nothing"
        );
        assert!(
            report.forward > 250_000,
            "reachability collapsed: {}",
            report.forward
        );
        assert!(
            report.bidirectional > report.forward - 1_000,
            "most of what can be reached must also be leavable"
        );
        // Highland reachability: the question the milestone was asked to answer.
        assert!(
            report.highland_is_reachable(),
            "no highland column is reachable on foot"
        );
        assert!(report.nearest_highland.is_some());
        // Barriers exist and are attributed.
        assert!(report.barriers.iter().sum::<u64>() > 0);
        let traversal_index = veldwake_combat::MoveBlockReason::ALL
            .iter()
            .position(|reason| *reason == veldwake_combat::MoveBlockReason::Traversal)
            .unwrap_or_default();
        assert!(
            report.barriers[traversal_index] > 0,
            "water blocked nothing anywhere in the region"
        );
        assert_eq!(
            report.barriers[0], 0,
            "a finite region produced a non-finite position"
        );
    }

    #[test]
    #[ignore = "searches the region for the adversary's clearing; about fourteen seconds"]
    fn the_derived_placement_is_the_locked_one() {
        let generator = generator();
        let grid = SurfaceGrid::sample(&generator);
        let report = audit(&grid, &movement(), (ROUTE_START_X, ROUTE_START_Z));
        let Some(host) = super::fight_host(generator.landmarks()) else {
            panic!("the golden composition has no landmark to host a fight");
        };
        let Some(placement) = place_adversary(
            &generator,
            &grid,
            &report,
            &PlacementRules::hosted_by(host.crown_column),
        ) else {
            panic!("the golden region offers no valid adversary placement");
        };
        assert_eq!(
            placement.column, ADVERSARY_COLUMN,
            "the placement moved; re-lock ADVERSARY_COLUMN deliberately with an OLD/NEW/WHY"
        );
    }

    #[test]
    fn the_adversary_stands_where_the_drawn_voxels_allow_a_fight() {
        // `the_adversary_stands_somewhere_the_region_allows_a_fight` asks the
        // placement predicates whether they still like their own answer, over
        // the `SurfaceGrid` the search itself walked. This asks the voxels.
        //
        // Nothing here reads `SurfaceGrid`, `is_level`, `is_clear_of_vegetation`
        // or the reachability report: the chunks are generated, the topmost
        // ground cell of every column in the disc is read out of them, and the
        // three placement rules are re-derived from what a viewer would see. If
        // the grid or a predicate ever drifted, this is the test that would not
        // drift with it.
        let generator = generator();
        let Some(host) = super::fight_host(generator.landmarks()) else {
            panic!("the golden composition has no landmark to host a fight");
        };
        let rules = PlacementRules::hosted_by(host.crown_column);
        let (centre_x, centre_z) = ADVERSARY_COLUMN;
        let radius = rules.level_radius.max(rules.clear_radius);
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));

        let mut top: HashMap<(i64, i64), i64> = HashMap::new();
        let mut wet: HashSet<(i64, i64)> = HashSet::new();
        let mut plants: Vec<(i64, i64, i64)> = Vec::new();

        for chunk_z in (centre_z - radius).div_euclid(edge)..=(centre_z + radius).div_euclid(edge) {
            for chunk_x in
                (centre_x - radius).div_euclid(edge)..=(centre_x + radius).div_euclid(edge)
            {
                // The region is three chunks tall, canopy included.
                for chunk_y in 0..3_i64 {
                    let coord = ChunkCoord::new(
                        i32::try_from(chunk_x).unwrap_or(i32::MAX),
                        i32::try_from(chunk_y).unwrap_or(i32::MAX),
                        i32::try_from(chunk_z).unwrap_or(i32::MAX),
                    );
                    let Some(chunk) = generator.generate(coord) else {
                        continue;
                    };
                    for local_z in 0..CHUNK_EDGE {
                        for local_x in 0..CHUNK_EDGE {
                            for local_y in 0..CHUNK_EDGE {
                                let Ok(local) =
                                    veldwake_voxel::LocalCoord::new(local_x, local_y, local_z)
                                else {
                                    continue;
                                };
                                let Some(material) =
                                    TerrainMaterial::from_voxel_id(chunk.read_local(local))
                                else {
                                    continue;
                                };
                                let x = chunk_x * edge + i64::try_from(local_x).unwrap_or(i64::MAX);
                                let z = chunk_z * edge + i64::try_from(local_z).unwrap_or(i64::MAX);
                                let y = chunk_y * edge + i64::try_from(local_y).unwrap_or(i64::MAX);
                                match material {
                                    TerrainMaterial::MeadowGrass
                                    | TerrainMaterial::HighlandGrass
                                    | TerrainMaterial::Soil
                                    | TerrainMaterial::Rock
                                    | TerrainMaterial::DeepRock
                                    | TerrainMaterial::Sediment => {
                                        let slot = top.entry((x, z)).or_insert(i64::MIN);
                                        *slot = (*slot).max(y);
                                    }
                                    TerrainMaterial::Water => {
                                        wet.insert((x, z));
                                    }
                                    TerrainMaterial::Trunk
                                    | TerrainMaterial::Foliage
                                    | TerrainMaterial::FoliageHighlight
                                    | TerrainMaterial::Shrub => plants.push((x, z, y)),
                                }
                            }
                        }
                    }
                }
            }
        }

        let Some(&floor) = top.get(&(centre_x, centre_z)) else {
            panic!("the adversary's own column has no ground voxel at all");
        };
        assert!(
            !wet.contains(&(centre_x, centre_z)),
            "the adversary stands in water"
        );

        // Exactly level, out to the level radius, with no water in the disc.
        let mut uneven = Vec::new();
        let mut flooded = Vec::new();
        let mut disc = 0_u32;
        for step_z in -rules.level_radius..=rules.level_radius {
            for step_x in -rules.level_radius..=rules.level_radius {
                if step_x * step_x + step_z * step_z > rules.level_radius * rules.level_radius {
                    continue;
                }
                let column = (centre_x + step_x, centre_z + step_z);
                disc += 1;
                if wet.contains(&column) {
                    flooded.push(column);
                }
                match top.get(&column) {
                    Some(&height) if height == floor => {}
                    other => uneven.push((column, other.copied())),
                }
            }
        }
        assert!(disc > 100, "the level disc is only {disc} columns");
        assert!(uneven.is_empty(), "the fight is on a terrace: {uneven:?}");
        assert!(
            flooded.is_empty(),
            "the fight disc holds water: {flooded:?}"
        );

        // Nothing growing in the clear disc, from the floor up.
        let mut growing = Vec::new();
        for (x, z, y) in plants {
            let (step_x, step_z) = (x - centre_x, z - centre_z);
            if step_x * step_x + step_z * step_z > rules.clear_radius * rules.clear_radius {
                continue;
            }
            if y > floor && y <= floor + rules.clear_height && growing.len() < 8 {
                growing.push((x, z, y));
            }
        }
        assert!(
            growing.is_empty(),
            "the adversary stands in vegetation: {growing:?}"
        );

        // And it is a walk. Chebyshev distance is a lower bound on the number
        // of eight-connected steps between two columns whatever the terrain
        // does, so this needs no graph, no audit and no route.
        let reach = (centre_x - ROUTE_START_X)
            .abs()
            .max((centre_z - ROUTE_START_Z).abs());
        let floor_steps = u32::try_from(reach).unwrap_or(u32::MAX);
        assert!(
            floor_steps >= rules.min_steps,
            "the adversary is at most {floor_steps} steps away, which is not a walk"
        );
    }

    #[test]
    fn the_adversary_stands_somewhere_the_region_allows_a_fight() {
        let derived = derive();
        let (x, z) = ADVERSARY_COLUMN;
        assert!(derived.grid.standable(x, z));
        assert!(!derived.grid.has_water(x, z));
        assert!(
            is_level(&derived.grid, x, z, 7),
            "the fight is on a terrace"
        );
        let Some(floor) = derived.grid.support_at(x, z) else {
            panic!("the adversary stands on nothing");
        };
        assert!(
            is_clear_of_vegetation(
                &derived.generator.vegetation(),
                x,
                z,
                i64::from(floor),
                7,
                16,
            ),
            "the adversary stands in a shrub"
        );
        // Reachable from the start, and far enough away to be a walk.
        let Some(steps) = derived.report.distance_to(&derived.grid, x, z) else {
            panic!("the adversary's column is not reachable from the route start");
        };
        let Some(host) = super::fight_host(derived.generator.landmarks()) else {
            panic!("the golden composition has no landmark to host a fight");
        };
        assert!(
            steps >= PlacementRules::hosted_by(host.crown_column).min_steps,
            "the walk is too short to be a walk: {steps} steps"
        );
    }

    #[test]
    fn the_route_is_a_walk_across_real_ground() {
        let derived = derive();
        let route = &derived.route;
        assert_eq!(route.start, (ROUTE_START_X, ROUTE_START_Z));
        assert_eq!(route.goal, ADVERSARY_COLUMN);
        assert!(route.columns.len() > 1);
        assert!(route.waypoints.len() >= 2);
        assert!(route.length > 0.0 && route.length.is_finite());

        // Every column of it is somewhere a body may be.
        for &(x, z) in &route.columns {
            assert!(
                derived.grid.standable(x, z),
                "the route passes through ({x}, {z}), which is not standable"
            );
        }
        // Consecutive columns are neighbours.
        for pair in route.columns.windows(2) {
            let [(ax, az), (bx, bz)] = [pair[0], pair[1]];
            assert!(
                (ax - bx).abs() <= 1 && (az - bz).abs() <= 1 && (ax, az) != (bx, bz),
                "the route jumps from ({ax}, {az}) to ({bx}, {bz})"
            );
        }
        // It leaves the chunk it started in, on both axes.
        let (x_crossings, z_crossings) = route.chunk_crossings();
        assert!(
            x_crossings > 0 && z_crossings > 0,
            "the route crosses {x_crossings} x boundaries and {z_crossings} z boundaries"
        );
        // Named checkpoints are on the route and in order.
        let mut previous = 0_u32;
        for checkpoint in &route.checkpoints {
            assert!(
                route.columns.contains(&checkpoint.column),
                "checkpoint {} is not on the route",
                checkpoint.name
            );
            assert!(
                checkpoint.steps >= previous,
                "checkpoint {} runs backwards",
                checkpoint.name
            );
            previous = checkpoint.steps;
            assert!(!checkpoint.intent.is_empty());
        }
        let names: Vec<&str> = route.checkpoints.iter().map(|c| c.name).collect();
        assert!(names.contains(&"route-start"));
        assert!(names.contains(&"route-goal"));
        assert!(names.contains(&"first-chunk-crossing"));
    }

    #[test]
    fn the_route_signature_is_locked() {
        let derived = derive();
        let measured = route_signature(&derived.generator, &movement(), &derived.route);
        assert_eq!(
            measured, GOLDEN_ROUTE_SIGNATURE,
            "the route moved; re-lock deliberately with an OLD/NEW/WHY paragraph"
        );
    }

    #[test]
    fn no_step_of_the_route_cuts_a_corner_between_two_blocked_columns() {
        // The classic eight-connected grid bug: a diagonal step accepted
        // between two columns a body may not stand in, so the route squeezes
        // through the corner of a rock or across the point of a river bend and
        // the runtime, which moves through the space rather than over the
        // lattice, cannot follow it.
        //
        // `check_move` already tests the whole diagonal as one intent, which is
        // why this holds; the test exists because "already holds" is exactly
        // what nobody notices stopping to hold.
        let derived = derive();
        let grid = &derived.grid;
        let mut diagonals = 0_u32;
        // A diagonal with one blocked shoulder is not this bug; it is a body
        // passing the corner of an obstacle, which the destination-only rule
        // of ADR-0008 allows and `the_runtime_accepts_every_step_of_the_route`
        // confirms the game actually walks. Counted, not asserted on, because
        // M8 gave the region its first obstacles and the number is worth
        // seeing move.
        let mut brushed = 0_u32;
        let mut cut = Vec::new();
        for pair in derived.route.columns.windows(2) {
            let [(from_x, from_z), (to_x, to_z)] = [pair[0], pair[1]];
            let (step_x, step_z) = (to_x - from_x, to_z - from_z);
            if step_x == 0 || step_z == 0 {
                continue;
            }
            diagonals += 1;
            let side_x = grid.standable(from_x + step_x, from_z);
            let side_z = grid.standable(from_x, from_z + step_z);
            if !side_x && !side_z {
                cut.push(((from_x, from_z), (to_x, to_z), side_x, side_z));
            }
            if !side_x || !side_z {
                brushed += 1;
            }
        }
        assert!(
            diagonals > 0,
            "a route with no diagonal step would make this test vacuous"
        );
        assert!(
            cut.is_empty(),
            "the route cuts a corner between columns a body may not occupy: {cut:?}"
        );
        // Branch QA re-derived it: 68 diagonal steps, exactly one of which
        // passes a blocked shoulder, and none with both shoulders blocked.
        assert!(
            brushed <= 1,
            "the route brushes {brushed} of {diagonals} obstacle corners, which is more than the one M8 measured"
        );
    }

    #[test]
    fn the_runtime_accepts_every_step_of_the_route_it_walks_continuously() {
        // The audit is a column graph and the route is a list of columns, but a
        // body walks through the points in between, where the ground query and
        // the column genuinely disagree
        // (`away_from_a_column_centre_the_ground_query_may_differ_but_only_within_two_voxels`).
        // So the audit is an upper bound, and this is the test that says the
        // bound is not loose on the one path the game actually ships: walk each
        // leg in walk-speed increments and put every single one to
        // `check_move` with the production adapters, not the cached grid.
        let derived = derive();
        let generator = &derived.generator;
        let ground = TerrainGround::new(generator);
        let veto = TerrainWalkability::new(generator);
        let movement = movement();
        let rules = veldwake_combat::MoveRules {
            movement: &movement,
            arena: None,
            ground: Some(&ground),
            legality: Some(&veto),
        };
        let per_tick = movement.speed() / 120.0;
        let mut samples = 0_u64;
        let mut refused = Vec::new();
        for pair in derived.route.waypoints.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let span = (to - from).length();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a leg is a few tens of units at a few hundredths per tick"
            )]
            let steps = ((span / per_tick).ceil() as u32).max(1);
            let mut previous = from;
            for step in 1..=steps {
                #[expect(clippy::cast_precision_loss, reason = "step counts are small")]
                let along = step as f32 / steps as f32;
                let point = from + (to - from) * along;
                samples += 1;
                if let Err(reason) = veldwake_combat::check_move(&rules, previous, point)
                    && refused.len() < 12
                {
                    refused.push((previous, point, reason.name()));
                }
                previous = point;
            }
        }
        assert!(
            samples > 1_000,
            "only {samples} continuous steps were put to the rules"
        );
        assert!(
            refused.is_empty(),
            "the runtime refuses a step of the route the audit accepted: {refused:?}"
        );
    }

    #[test]
    fn the_route_is_walkable_in_a_real_encounter() {
        // The proof that matters. A settled-column breadth-first search is a
        // topological claim; this drives the authoritative tick loop with the
        // real pelvis filter, the real separation and a real adversary, and
        // asks whether a body actually gets there.
        let derived = derive();
        let generator = &derived.generator;
        let ground = TerrainGround::new(generator);
        let veto = TerrainWalkability::new(generator);
        let world = WorldContact::terrain(&ground, &veto);

        let mut setup = fixture::golden_setup();
        setup.arena = None;
        setup.player_victory = PlayerVictoryPolicy::Remain;
        setup.starts = [
            column_centre(derived.route.start.0, derived.route.start.1),
            column_centre(derived.route.goal.0, derived.route.goal.1),
        ];
        let mut encounter = match Encounter::new(&setup, Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("the traversal encounter must build: {error}"),
        };
        encounter.arm();

        let speed = movement().speed();
        let per_tick = speed / 120.0;
        let mut target = 1_usize;
        let mut arrived = false;
        let mut woke_at = None;
        let mut blocked = 0_u32;
        let mut previous = encounter.combatant(Side::Player).position();
        // Generous: four times the ideal walk, so a stall shows as a failure to
        // arrive rather than as a timeout nobody can read.
        let limit = ((derived.route.length / per_tick) * 4.0) as u64 + 2_400;

        for tick in 0..limit {
            let position = encounter.combatant(Side::Player).position();
            let Some(waypoint) = derived.route.waypoints.get(target) else {
                arrived = true;
                break;
            };
            let to_waypoint = *waypoint - position;
            if to_waypoint.length() < 0.35 {
                target += 1;
                continue;
            }
            let intent = to_waypoint.normalize_or_zero();
            let _ = encounter.step(Intent::player(intent, false, false), world);

            let position = encounter.combatant(Side::Player).position();
            assert!(position.is_finite(), "the player left the number line");
            assert!(
                veto.walkable(f64::from(position.x), f64::from(position.y)),
                "the player entered water or left the region at {position}"
            );
            let moved = (position - previous).length();
            if woke_at.is_none() {
                // Traversal movement is bounded by the walk speed. Once the
                // adversary is awake this stops being a claim about walking:
                // knockback is a bounded kinematic displacement of up to `0.30`
                // world units applied inside one tick, and a separation pushes
                // a body further still. Those are combat consequences, and
                // asserting a walk speed against them would be asserting the
                // wrong thing.
                assert!(
                    moved <= per_tick * 1.5,
                    "the player moved {moved} in one walking tick, over the speed bound"
                );
                if moved < per_tick * 0.05 {
                    blocked += 1;
                }
            }
            previous = position;

            if woke_at.is_none()
                && encounter.brain().state() != veldwake_combat::AdversaryState::Idle
            {
                woke_at = Some(tick);
            }
        }

        assert!(
            arrived,
            "the body never finished the route the audit says is walkable; \
             reached waypoint {target} of {}, {blocked} stalled ticks",
            derived.route.waypoints.len()
        );
        let finished = encounter.combatant(Side::Player).position();
        let goal = column_centre(derived.route.goal.0, derived.route.goal.1);
        assert!(
            (finished - goal).length() < 12.0,
            "the body finished {} from the goal",
            (finished - goal).length()
        );
        assert!(
            woke_at.is_some(),
            "walking up to the adversary never woke it"
        );
    }

    #[test]
    #[ignore = "writes the whole-region reachability report; run it deliberately"]
    fn the_region_reachability_report() {
        let derived = derive();
        let report = &derived.report;
        let grid = &derived.grid;
        println!("--- M7 reachability audit, golden region ---");
        println!(
            "grid {} x {} = {} columns   sample {} ms   audit {} ms   route+placement {} ms",
            grid.columns_x(),
            grid.columns_z(),
            report.columns_total,
            derived.sample_ms,
            derived.audit_ms,
            derived.route_ms
        );
        println!(
            "with ground {}   standable {}   start {:?}",
            report.columns_with_ground, report.columns_standable, report.start
        );
        #[expect(clippy::cast_precision_loss, reason = "a percentage in a report")]
        let percent =
            |part: usize| (part as f64) * 100.0 / (report.columns_standable.max(1) as f64);
        println!(
            "forward {} ({:.2}% of standable)   reverse {}   bidirectional {}   one-way {}",
            report.forward,
            percent(report.forward),
            report.reverse,
            report.bidirectional,
            report.one_way
        );
        println!(
            "largest symmetric components: {:?}",
            report.component_sizes.iter().take(8).collect::<Vec<_>>()
        );
        for (index, reason) in veldwake_combat::MoveBlockReason::ALL.iter().enumerate() {
            println!("  barrier {:<16} {}", reason.name(), report.barriers[index]);
        }
        println!(
            "highland standable {}   reachable {}   nearest {:?}",
            report.highland_standable, report.highland_reachable, report.nearest_highland
        );
        println!("locked placement {ADVERSARY_COLUMN:?}");
        let route = &derived.route;
        println!(
            "route {:?} -> {:?}   {} columns   {} waypoints   {:.2} world units   {:.1} s at {:.2} u/s",
            route.start,
            route.goal,
            route.columns.len(),
            route.waypoints.len(),
            route.length,
            route.duration_at(movement().speed()),
            movement().speed()
        );
        let (x_crossings, z_crossings) = route.chunk_crossings();
        println!("chunk crossings: x {x_crossings}   z {z_crossings}");
        for checkpoint in &route.checkpoints {
            println!(
                "  checkpoint {:<22} {:?} at {} steps — {}",
                checkpoint.name, checkpoint.column, checkpoint.steps, checkpoint.intent
            );
        }
        print!("waypoints:");
        for waypoint in &route.waypoints {
            print!(" {:.1},{:.1}", waypoint.x, waypoint.y);
        }
        println!();
        println!(
            "GOLDEN_ROUTE_SIGNATURE measured: {:#018x}",
            route_signature(&derived.generator, &movement(), route)
        );

        println!("--- M8 landmarks, as the audit sees them ---");
        let plan = derived.generator.landmarks();
        let overlook = plan.overlook();
        println!(
            "overlook {:?} ground {} clearing {}",
            overlook.column, overlook.ground_face, overlook.clearing_radius
        );
        for instance in plan.instances() {
            let (x, z) = instance.crown_column;
            // How close a body can get, and how far it has to walk to do it.
            let mut nearest = None;
            for radius in 1..=24_i64 {
                for dz in -radius..=radius {
                    for dx in -radius..=radius {
                        if dx.abs().max(dz.abs()) != radius {
                            continue;
                        }
                        let (cx, cz) = (x + dx, z + dz);
                        if !grid.standable(cx, cz) {
                            continue;
                        }
                        if let Some(steps) = report.distance_to(grid, cx, cz) {
                            nearest = Some(((cx, cz), steps, radius));
                            break;
                        }
                    }
                    if nearest.is_some() {
                        break;
                    }
                }
                if nearest.is_some() {
                    break;
                }
            }
            println!(
                "  {} {:<6} at {:?}  approach {:?}",
                instance.role.name(),
                instance.descriptor.class.name(),
                instance.crown_column,
                nearest
            );
        }
        if let Some(host) = super::fight_host(plan) {
            println!(
                "fight host: landmark {} ({}) at {:?}",
                host.index,
                host.descriptor.class.name(),
                host.crown_column
            );
            let hosted = place_adversary(
                &derived.generator,
                grid,
                report,
                &PlacementRules::hosted_by(host.crown_column),
            );
            println!("hosted placement {hosted:?}");
        }
        let _ = Vec2::ZERO;
    }

    // -----------------------------------------------------------------------
    // M8: landmarks are solid
    // -----------------------------------------------------------------------

    #[test]
    fn the_keep_out_is_the_widest_body_the_world_carries() {
        // The keep-out is stated as a number so the grid and the runtime veto
        // cannot disagree, and measured here so the number cannot drift away
        // from the bodies it is supposed to be about.
        let mut compiler = veldwake_character::CharacterCompiler::new();
        let mut widest: f64 = 0.0;
        let mut tallest: f64 = 0.0;
        for descriptor in [
            fixture::player_descriptor(),
            fixture::adversary_descriptor(),
        ] {
            let character = match compiler.compile_descriptor(&descriptor) {
                Ok(character) => character,
                Err(error) => panic!("a fixture body does not compile: {error}"),
            };
            let capsule = character.collision().capsule();
            widest = widest.max(f64::from(capsule.radius));
            tallest = tallest.max(f64::from(capsule.total_height()));
        }
        assert!(
            (widest..widest + 0.05).contains(&BODY_KEEP_OUT_RADIUS),
            "the keep-out radius is {BODY_KEEP_OUT_RADIUS}, the widest body is {widest:.4}"
        );
        assert!(
            (tallest..tallest + 0.05).contains(&BODY_KEEP_OUT_HEIGHT),
            "the keep-out height is {BODY_KEEP_OUT_HEIGHT}, the tallest body is {tallest:.4}"
        );
    }

    #[test]
    fn a_body_cannot_walk_into_a_landmark() {
        // The whole point of M8E: a visible wall is a wall. Asked of the
        // runtime veto and of the cached grid, at the crown column of each
        // landmark, which is solid stone from the ground up.
        let generator = generator();
        let grid = SurfaceGrid::sample(&generator);
        let veto = TerrainWalkability::new(&generator);
        let cached = grid.legality();
        for instance in generator.landmarks().instances() {
            // Not the crown column: a gate's tallest column is the lintel
            // over its opening, and the ground under an opening is meant to
            // be walkable. The witness is a column filled from the base
            // course up, which is what a wall is.
            let (low_x, high_x, low_z, high_z) = instance.bounds;
            let Some((x, z)) = (low_z..=high_z)
                .flat_map(|z| (low_x..=high_x).map(move |x| (x, z)))
                .find(|(x, z)| {
                    instance
                        .column(*x, *z)
                        .is_some_and(|(low, _)| low <= instance.base_y)
                })
            else {
                panic!("landmark {} has no grounded column", instance.index);
            };
            let centre = column_centre(x, z);
            let (cx, cz) = (f64::from(centre.x), f64::from(centre.y));
            assert!(
                !veto.walkable(cx, cz),
                "a body may walk into landmark {} at ({x}, {z})",
                instance.index
            );
            assert!(
                !cached.walkable(cx, cz),
                "the cached grid lets a body into landmark {} at ({x}, {z})",
                instance.index
            );
            assert!(
                !grid.standable(x, z),
                "landmark {} is a place to stand",
                instance.index
            );
        }
    }

    #[test]
    fn the_keep_out_reaches_past_the_stone_by_a_body_radius_and_no_further() {
        // A keep-out that stopped at the stone would let a body's shoulder
        // into the wall; one that reached far would fence off the approach.
        // Both failures are visible in the same scan.
        let generator = generator();
        let plan = generator.landmarks();
        let veto = TerrainWalkability::new(&generator);
        let instance = &plan.instances()[0];
        let (low_x, high_x, low_z, high_z) = instance.bounds;
        let (mut blocked_outside, mut free_at_reach) = (0_u64, 0_u64);
        let margin = BODY_KEEP_OUT_RADIUS.ceil() as i64 + 2;
        for z in (low_z - margin)..=(high_z + margin) {
            for x in (low_x - margin)..=(high_x + margin) {
                if plan.column(x, z).is_some() {
                    continue;
                }
                let centre = column_centre(x, z);
                let (cx, cz) = (f64::from(centre.x), f64::from(centre.y));
                if veto.walkable(cx, cz) {
                    continue;
                }
                // Refused and not stone: it must be within a body radius of
                // stone, or the water veto must be what refused it.
                let near = (-2..=2).any(|dz| {
                    (-2..=2).any(|dx| {
                        plan.column(x + dx, z + dz).is_some_and(|_| {
                            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
                            let (fx, fz) = (dx as f64, dz as f64);
                            fx.hypot(fz) <= BODY_KEEP_OUT_RADIUS + 1.5
                        })
                    })
                });
                assert!(
                    near,
                    "({x}, {z}) is refused and is nowhere near landmark {}",
                    instance.index
                );
                blocked_outside += 1;
            }
        }
        // And the ring two columns out is open, so the landmark can be walked
        // around and up to.
        for z in [low_z - 3, high_z + 3] {
            for x in (low_x - 3)..=(high_x + 3) {
                let centre = column_centre(x, z);
                if veto.walkable(f64::from(centre.x), f64::from(centre.y)) {
                    free_at_reach += 1;
                }
            }
        }
        assert!(blocked_outside > 0, "the keep-out does not reach the stone");
        assert!(
            free_at_reach > 0,
            "the landmark cannot be approached from any side"
        );
    }

    #[test]
    fn the_gate_is_a_gate_and_a_body_can_walk_through_it() {
        // The owner's condition for the class: a gate that cannot be walked
        // through is an arch, and this milestone did not build an arch. The
        // proof is a walk, not a drawing: a breadth-first search over the
        // columns around the gate, refused to leave a narrow corridor, has to
        // cross from one side to the other, and the corridor is only wide
        // enough for the opening.
        let generator = generator();
        let plan = generator.landmarks();
        let grid = SurfaceGrid::sample(&generator);
        let movement = movement();
        let Some(gate) = plan
            .instances()
            .iter()
            .find(|one| one.descriptor.class.name() == "gate")
        else {
            panic!("the golden world has no gate");
        };
        let (low_x, high_x, low_z, high_z) = gate.bounds;
        // The gate spans one axis; the walk crosses the other.
        let across_x = (high_x - low_x) < (high_z - low_z);
        let corridor = 3_i64;
        let (start, goal) = if across_x {
            (
                (
                    (low_x + high_x) / 2 - (high_x - low_x) / 2 - corridor,
                    (low_z + high_z) / 2,
                ),
                (
                    (low_x + high_x) / 2 + (high_x - low_x) / 2 + corridor,
                    (low_z + high_z) / 2,
                ),
            )
        } else {
            (
                (
                    (low_x + high_x) / 2,
                    (low_z + high_z) / 2 - (high_z - low_z) / 2 - corridor,
                ),
                (
                    (low_x + high_x) / 2,
                    (low_z + high_z) / 2 + (high_z - low_z) / 2 + corridor,
                ),
            )
        };
        let report = audit(&grid, &movement, start);
        let Some(steps) = report.distance_to(&grid, goal.0, goal.1) else {
            panic!(
                "no walk crosses the gate at ({}, {}) -> ({}, {})",
                start.0, start.1, goal.0, goal.1
            );
        };
        let Some(path) = report.path_to(&grid, goal.0, goal.1) else {
            panic!("the crossing has no path");
        };
        // The walk has to pass under the gate's own footprint, not around it.
        let under = path
            .iter()
            .any(|(x, z)| (low_x..=high_x).contains(x) && (low_z..=high_z).contains(z));
        assert!(
            under,
            "the walk of {steps} steps goes around the gate instead of through it: {path:?}"
        );
        // And what it walks under is the gate's opening: stone overhead.
        let crossing: Vec<(i64, i64)> = path
            .iter()
            .copied()
            .filter(|(x, z)| (low_x..=high_x).contains(x) && (low_z..=high_z).contains(z))
            .collect();
        for (x, z) in &crossing {
            let Some((stone_low, _)) = gate.column(*x, *z) else {
                continue;
            };
            let Some(floor) = grid.support_at(*x, *z) else {
                panic!("the opening has no ground at ({x}, {z})");
            };
            let clearance = stone_low - i64::from(floor);
            #[expect(clippy::cast_precision_loss, reason = "region heights")]
            let tall_enough = clearance as f64 >= BODY_KEEP_OUT_HEIGHT;
            assert!(
                tall_enough,
                "the gate's opening is {clearance} voxels tall at ({x}, {z})"
            );
        }
        assert!(
            !crossing.is_empty(),
            "the walk crosses the gate without entering its footprint"
        );
    }

    #[test]
    fn the_cached_grid_agrees_with_the_runtime_veto_around_every_landmark() {
        // The agreement test walks the water chunks; a landmark is elsewhere,
        // and the keep-out is the newest reason for the two to drift apart.
        let generator = generator();
        let grid = SurfaceGrid::sample(&generator);
        let veto = TerrainWalkability::new(&generator);
        let cached = grid.legality();
        let mut checked = 0_u64;
        for instance in generator.landmarks().instances() {
            let (low_x, high_x, low_z, high_z) = instance.bounds;
            for z in (low_z - 4)..=(high_z + 4) {
                for x in (low_x - 4)..=(high_x + 4) {
                    let centre = column_centre(x, z);
                    let (cx, cz) = (f64::from(centre.x), f64::from(centre.y));
                    assert_eq!(
                        cached.walkable(cx, cz),
                        veto.walkable(cx, cz),
                        "the cached grid disagrees about landmark {} at ({x}, {z})",
                        instance.index
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 700, "only {checked} columns were compared");
    }

    #[test]
    fn a_landmark_blocks_the_runtime_rule_and_not_only_the_audit() {
        // `check_move` is what the game asks. A step whose destination is
        // inside a landmark has to come back refused, with the traversal
        // reason, exactly as a step into the river does.
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let veto = TerrainWalkability::new(&generator);
        let movement = movement();
        let rules = veldwake_combat::MoveRules {
            movement: &movement,
            arena: None,
            ground: Some(&ground),
            legality: Some(&veto),
        };
        let instance = &generator.landmarks().instances()[0];
        let (x, z) = instance.crown_column;
        let inside = column_centre(x, z);
        // A body one column outside the stone, stepping straight at it.
        let (low_x, _, _, _) = instance.bounds;
        let outside = column_centre(low_x - 2, z);
        let outcome = veldwake_combat::check_move(&rules, outside, inside);
        assert_eq!(
            outcome,
            Err(veldwake_combat::MoveBlockReason::Traversal),
            "a step into landmark {} was answered with {outcome:?}",
            instance.index
        );
    }

    #[test]
    fn the_route_starts_at_the_world_overlook() {
        let generator = generator();
        let overlook = generator.landmarks().overlook();
        assert_eq!(
            (ROUTE_START_X, ROUTE_START_Z),
            overlook.column,
            "the session begins somewhere the world did not compose"
        );
    }

    #[test]
    fn the_runtime_walks_through_the_gate_and_not_only_the_audit() {
        // `the_gate_is_a_gate_and_a_body_can_walk_through_it` is a column
        // graph, which is an upper bound like every other audit claim. This
        // is the same crossing put to the production adapters in walk-speed
        // increments, so "the opening admits the body" is a statement about
        // the rule the game runs rather than about the lattice.
        let generator = generator();
        let grid = SurfaceGrid::sample(&generator);
        let movement = movement();
        let plan = generator.landmarks();
        let Some(gate) = plan
            .instances()
            .iter()
            .find(|one| one.descriptor.class == veldwake_procedural::SilhouetteClass::Gate)
        else {
            panic!("the golden world has no gate");
        };
        let (low_x, high_x, low_z, high_z) = gate.bounds;
        let across_x = (high_x - low_x) < (high_z - low_z);
        let corridor = 3_i64;
        let (start, goal) = if across_x {
            (
                (low_x - corridor, (low_z + high_z) / 2),
                (high_x + corridor, (low_z + high_z) / 2),
            )
        } else {
            (
                ((low_x + high_x) / 2, low_z - corridor),
                ((low_x + high_x) / 2, high_z + corridor),
            )
        };
        let report = audit(&grid, &movement, start);
        let Some(path) = report.path_to(&grid, goal.0, goal.1) else {
            panic!("no walk crosses the gate from {start:?} to {goal:?}");
        };

        let ground = TerrainGround::new(&generator);
        let veto = TerrainWalkability::new(&generator);
        let rules = veldwake_combat::MoveRules {
            movement: &movement,
            arena: None,
            ground: Some(&ground),
            legality: Some(&veto),
        };
        let per_tick = movement.speed() / 120.0;
        let mut samples = 0_u64;
        let mut inside = 0_u64;
        let mut refused = Vec::new();
        for pair in path.windows(2) {
            let (from, to) = (
                column_centre(pair[0].0, pair[0].1),
                column_centre(pair[1].0, pair[1].1),
            );
            let span = (to - from).length();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a step between adjacent columns is about one unit"
            )]
            let steps = ((span / per_tick).ceil() as u32).max(1);
            let mut previous = from;
            for step in 1..=steps {
                #[expect(clippy::cast_precision_loss, reason = "step counts are small")]
                let along = step as f32 / steps as f32;
                let point = from + (to - from) * along;
                samples += 1;
                if (low_x..=high_x).contains(&pair[1].0) && (low_z..=high_z).contains(&pair[1].1) {
                    inside += 1;
                }
                if let Err(reason) = veldwake_combat::check_move(&rules, previous, point)
                    && refused.len() < 8
                {
                    refused.push((previous, point, reason.name()));
                }
                previous = point;
            }
        }
        assert!(samples > 100, "only {samples} samples crossed the gate");
        assert!(
            inside > 0,
            "the continuous walk never entered the gate's own footprint"
        );
        assert!(
            refused.is_empty(),
            "the runtime refused {} of {samples} steps through the gate: {refused:?}",
            refused.len()
        );
    }

    #[test]
    fn qa_the_keep_out_agrees_with_the_voxels_a_viewer_can_see() {
        // An oracle built from the drawn chunks rather than from the plan:
        // read every landmark voxel out of the generated world, and ask the
        // two questions that matter in both directions. A body may never
        // stand where its own volume would hold stone, and the veto may never
        // refuse ground that is further from stone than a body is wide.
        let generator = generator();
        let veto = TerrainWalkability::new(&generator);
        let field = generator.field();
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));

        for instance in generator.landmarks().instances() {
            let (low_x, high_x, low_z, high_z) = instance.bounds;
            let (low_y, high_y) = instance.vertical_bounds();

            // The solid set, read out of the chunks the renderer draws.
            let mut solid: HashSet<(i64, i64, i64)> = HashSet::new();
            for chunk_z in low_z.div_euclid(edge)..=high_z.div_euclid(edge) {
                for chunk_y in low_y.div_euclid(edge)..=high_y.div_euclid(edge) {
                    for chunk_x in low_x.div_euclid(edge)..=high_x.div_euclid(edge) {
                        let (Ok(cx), Ok(cy), Ok(cz)) = (
                            i32::try_from(chunk_x),
                            i32::try_from(chunk_y),
                            i32::try_from(chunk_z),
                        ) else {
                            continue;
                        };
                        let Some(chunk) = generator.generate(ChunkCoord::new(cx, cy, cz)) else {
                            continue;
                        };
                        for lz in 0..CHUNK_EDGE {
                            for ly in 0..CHUNK_EDGE {
                                for lx in 0..CHUNK_EDGE {
                                    let Ok(cell) = veldwake_voxel::LocalCoord::new(lx, ly, lz)
                                    else {
                                        continue;
                                    };
                                    let id = chunk.read_local(cell);
                                    if veldwake_procedural::LandmarkMaterial::from_voxel_id(id)
                                        .is_none()
                                    {
                                        continue;
                                    }
                                    solid.insert((
                                        chunk_x * edge + i64::try_from(lx).unwrap_or_default(),
                                        chunk_y * edge + i64::try_from(ly).unwrap_or_default(),
                                        chunk_z * edge + i64::try_from(lz).unwrap_or_default(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            assert!(
                solid.len() > 100,
                "landmark {} drew only {} voxels",
                instance.index,
                solid.len()
            );

            // Sampled at quarter columns, including negative coordinates and
            // positions that are nowhere near a column centre.
            let mut refused_with_stone = 0_u64;
            let mut allowed = 0_u64;
            let reach = 4_i64;
            for step_z in ((low_z - reach) * 4)..=((high_z + reach) * 4) {
                for step_x in ((low_x - reach) * 4)..=((high_x + reach) * 4) {
                    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
                    let (x, z) = (step_x as f64 / 4.0, step_z as f64 / 4.0);
                    let walkable = veto.walkable(x, z);
                    let floor = field.sample(x, z).surface_y().saturating_add(1);
                    #[expect(clippy::cast_precision_loss, reason = "region heights")]
                    let floor_f = floor as f64;

                    // Does the body's own volume hold drawn stone here?
                    let mut holds_stone = false;
                    let radius = BODY_KEEP_OUT_RADIUS;
                    let top = floor_f + BODY_KEEP_OUT_HEIGHT;
                    for column_z in (z - radius).floor() as i64..=(z + radius).floor() as i64 {
                        for column_x in (x - radius).floor() as i64..=(x + radius).floor() as i64 {
                            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
                            let (cx, cz) = (column_x as f64, column_z as f64);
                            let dx = (cx - x).max(x - (cx + 1.0)).max(0.0);
                            let dz = (cz - z).max(z - (cz + 1.0)).max(0.0);
                            if dx * dx + dz * dz > radius * radius {
                                continue;
                            }
                            for y in floor..=(floor + 3) {
                                #[expect(clippy::cast_precision_loss, reason = "region heights")]
                                let (voxel_low, voxel_high) = (y as f64, (y + 1) as f64);
                                if voxel_low < top
                                    && voxel_high > floor_f
                                    && solid.contains(&(column_x, y, column_z))
                                {
                                    holds_stone = true;
                                }
                            }
                        }
                    }

                    if holds_stone {
                        assert!(
                            !walkable,
                            "a body at ({x}, {z}) would hold drawn stone and traversal allows it"
                        );
                        refused_with_stone += 1;
                    } else if walkable {
                        allowed += 1;
                    }
                }
            }
            assert!(
                refused_with_stone > 100,
                "landmark {} produced only {refused_with_stone} refusals against drawn stone",
                instance.index
            );
            assert!(
                allowed > 100,
                "landmark {} fences off everything around it: only {allowed} free samples",
                instance.index
            );
        }
    }

    #[test]
    fn qa_a_real_encounter_walks_a_body_through_the_gate_and_into_its_pillars() {
        // The owner's condition for the class, put to the authoritative loop
        // rather than to a column graph or to `check_move` alone: an encounter
        // with the real ground, the real veto, the real pelvis filter and the
        // real capsule, driven tick by tick with nothing but a walk intent.
        // It has to come out the other side of the opening, and it has to fail
        // to come out the other side of a pillar.
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let veto = TerrainWalkability::new(&generator);
        let world = WorldContact::terrain(&ground, &veto);
        let plan = generator.landmarks();
        let Some(gate) = plan
            .instances()
            .iter()
            .find(|one| one.descriptor.class == veldwake_procedural::SilhouetteClass::Gate)
        else {
            panic!("the golden world has no gate");
        };
        let (low_x, high_x, low_z, high_z) = gate.bounds;
        // The gate spans one axis; a body crosses the other.
        let across_x = (high_x - low_x) < (high_z - low_z);
        let (centre_x, centre_z) = ((low_x + high_x) / 2, (low_z + high_z) / 2);

        // Where the opening is: the columns whose stone starts above a body.
        let mut opening: Vec<(i64, i64)> = Vec::new();
        for z in low_z..=high_z {
            for x in low_x..=high_x {
                let Some((stone_low, _)) = gate.column(x, z) else {
                    continue;
                };
                if stone_low > gate.base_y {
                    opening.push((x, z));
                }
            }
        }
        assert!(!opening.is_empty(), "the gate has no opening columns");
        let through = opening[opening.len() / 2];

        // One walk through the opening, and one into a pillar: same machinery,
        // same distance, opposite expectations.
        let run = |start: (i64, i64), goal: (i64, i64)| -> (f32, bool) {
            let mut setup = fixture::golden_setup();
            setup.arena = None;
            setup.player_victory = PlayerVictoryPolicy::Remain;
            // The adversary is parked far away: this test is about stone.
            setup.starts = [
                column_centre(start.0, start.1),
                column_centre(start.0, start.1 + 200),
            ];
            let mut encounter = match Encounter::new(&setup, Some(&ground)) {
                Ok(encounter) => encounter,
                Err(error) => panic!("the gate encounter must build: {error}"),
            };
            encounter.arm();
            let target = column_centre(goal.0, goal.1);
            let mut crossed = false;
            for _ in 0..2_400_u32 {
                let position = encounter.combatant(Side::Player).position();
                let to_goal = target - position;
                if to_goal.length() < 0.4 {
                    break;
                }
                let _ = encounter.step(
                    Intent::player(to_goal.normalize_or_zero(), false, false),
                    world,
                );
                let position = encounter.combatant(Side::Player).position();
                assert!(position.is_finite(), "the player left the number line");
                #[expect(clippy::cast_possible_truncation, reason = "region coordinates")]
                let column = (position.x.floor() as i64, position.y.floor() as i64);
                if (low_x..=high_x).contains(&column.0) && (low_z..=high_z).contains(&column.1) {
                    crossed = true;
                }
                // Whatever happens, the body may never stand in stone.
                assert!(
                    veto.walkable(f64::from(position.x), f64::from(position.y)),
                    "the body stood at {position}, which traversal refuses"
                );
            }
            let position = encounter.combatant(Side::Player).position();
            ((position - target).length(), crossed)
        };

        let (before, after) = if across_x {
            ((low_x - 6, through.1), (high_x + 6, through.1))
        } else {
            ((through.0, low_z - 6), (through.0, high_z + 6))
        };
        let (remaining, crossed) = run(before, after);
        assert!(
            crossed,
            "the body reached {remaining:.2} from the far side without entering the gate"
        );
        assert!(
            remaining < 1.0,
            "the body stopped {remaining:.2} short of the far side of the opening"
        );

        // And the same walk aimed at a pillar instead of the opening. A pillar
        // column is one whose stone starts at the base course.
        let mut pillar = None;
        for z in low_z..=high_z {
            for x in low_x..=high_x {
                if gate.column(x, z).is_some_and(|(low, _)| low <= gate.base_y) {
                    pillar = Some((x, z));
                }
            }
        }
        let Some(pillar) = pillar else {
            panic!("the gate has no grounded pillar");
        };
        let (blocked_from, blocked_to) = if across_x {
            ((pillar.0 - 6, pillar.1), (pillar.0 + 6, pillar.1))
        } else {
            ((pillar.0, pillar.1 - 6), (pillar.0, pillar.1 + 6))
        };
        let (stopped, _) = run(blocked_from, blocked_to);
        assert!(
            stopped > 1.0,
            "a body walked straight through a pillar and stopped {stopped:.2} from the far side"
        );
        let _ = (centre_x, centre_z);
    }
}
