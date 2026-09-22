//! The landmark plan: a finite, deterministic, immutable description of every
//! landmark in one world.
//!
//! Derived once, before any chunk is generated, from the world identity and
//! the terrain field alone. It is pure — no chunk, no camera, no session, no
//! combat, no order dependence — and it is fallible, which is why it is built
//! when the world is built rather than lazily on the first chunk: a chunk
//! generator has nowhere to put a planning failure.
//!
//! What the plan is not: a spawn system, a quest, a save, a navigation graph
//! or an entity. It is where three pieces of world geometry stand and what
//! they are made of.

use std::collections::BTreeSet;

use veldwake_voxel::CHUNK_EDGE;

use crate::hash::fnv1a64;
use crate::identity::{StreamLabel, WorldIdentity};
use crate::landmark::compile::CompiledMonolith;
use crate::landmark::descriptor::{MonolithDescriptor, SilhouetteClass, SpanAxis};
use crate::landmark::material::LandmarkMaterial;
use crate::landmark::visibility::{Observer, VisibleBand, WorldOccluders};
use crate::landmark::{LANDMARK_PLAN_VERSION, LandmarkControls};
use crate::noise::{hash_2d, sub_hash};
use crate::terrain::TerrainField;
use crate::vegetation::VegetationSystem;

/// What a landmark is for, in the discovery composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LandmarkRole {
    /// Visible from the overlook: one of the two the first choice is between.
    FirstChoice,
    /// Hidden from the overlook and found from one of the first two.
    Revealed,
}

impl LandmarkRole {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FirstChoice => "first-choice",
            Self::Revealed => "revealed",
        }
    }
}

/// Ground a landmark plan keeps free of plants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reservation {
    /// A clearing: a plant anchored inside the disc is not grown.
    Clearing { centre: (i64, i64), radius: i64 },
    /// A structure and its apron: a plant whose horizontal bounds touch the
    /// rectangle is not grown, so nothing can lean into the stone.
    Footprint {
        min_x: i64,
        max_x: i64,
        min_z: i64,
        max_z: i64,
    },
}

impl Reservation {
    /// Whether this reservation removes a plant with the given anchor and
    /// horizontal bounds.
    #[must_use]
    pub const fn suppresses(
        &self,
        anchor_x: i64,
        anchor_z: i64,
        min_x: i64,
        max_x: i64,
        min_z: i64,
        max_z: i64,
    ) -> bool {
        match *self {
            Self::Clearing { centre, radius } => {
                let dx = anchor_x - centre.0;
                let dz = anchor_z - centre.1;
                dx * dx + dz * dz <= radius * radius
            }
            Self::Footprint {
                min_x: low_x,
                max_x: high_x,
                min_z: low_z,
                max_z: high_z,
            } => min_x <= high_x && max_x >= low_x && min_z <= high_z && max_z >= low_z,
        }
    }

    /// Whether any reservation in a set removes the plant.
    #[must_use]
    pub fn any_suppresses(
        reservations: &[Self],
        anchor_x: i64,
        anchor_z: i64,
        min_x: i64,
        max_x: i64,
        min_z: i64,
        max_z: i64,
    ) -> bool {
        reservations.iter().any(|reservation| {
            reservation.suppresses(anchor_x, anchor_z, min_x, max_x, min_z, max_z)
        })
    }
}

/// Where a session looks out over the world for the first time.
///
/// A compositional property of the world, not a player spawn: the world has
/// no idea a player exists. The client chooses to start a session here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryOverlook {
    pub column: (i64, i64),
    /// The face a sole rests on at the overlook.
    pub ground_face: i32,
    pub clearing_radius: i64,
}

/// One landmark, placed.
#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkInstance {
    pub index: usize,
    pub role: LandmarkRole,
    pub descriptor: MonolithDescriptor,
    pub compiled: CompiledMonolith,
    /// World column of the compiled grid's local origin.
    pub origin: (i64, i64),
    /// World `y` of the base course.
    pub base_y: i64,
    /// The column the silhouette is judged from: the tallest one.
    pub crown_column: (i64, i64),
    /// Inclusive horizontal bounds of the structure itself.
    pub bounds: (i64, i64, i64, i64),
    /// The structure plus its apron, kept free of plants.
    pub reservation: Reservation,
    /// Which landmark this one is discovered from, for the revealed instance.
    pub revealed_from: Option<usize>,
    /// The band of silhouette the world proxy left visible where it matters.
    pub visible: VisibleBand,
    /// Terrain face under every footprint column, for the foundation rule.
    ground: Vec<i32>,
}

impl LandmarkInstance {
    #[must_use]
    pub const fn class(&self) -> SilhouetteClass {
        self.descriptor.class
    }

    /// Whether a chunk's horizontal extent can hold any of this landmark.
    #[must_use]
    pub const fn touches_columns(&self, min_x: i64, max_x: i64, min_z: i64, max_z: i64) -> bool {
        let (low_x, high_x, low_z, high_z) = self.bounds;
        min_x <= high_x && max_x >= low_x && min_z <= high_z && max_z >= low_z
    }

    fn local_column(&self, x: i64, z: i64) -> Option<(usize, usize)> {
        let (size_x, _, size_z) = self.compiled.size();
        let lx = usize::try_from(x - self.origin.0).ok()?;
        let lz = usize::try_from(z - self.origin.1).ok()?;
        (lx < size_x && lz < size_z).then_some((lx, lz))
    }

    fn ground_face(&self, lx: usize, lz: usize) -> i32 {
        let (size_x, _, _) = self.compiled.size();
        self.ground
            .get(lz * size_x + lx)
            .copied()
            .unwrap_or(i32::MAX)
    }

    /// The material this landmark puts at a world voxel, if any.
    ///
    /// Includes the foundation: a footprint column whose terrain sits below
    /// the base course is filled in stone from its own surface up, so a
    /// landmark never floats and never replaces a terrain voxel.
    #[must_use]
    pub fn material_at(&self, x: i64, y: i64, z: i64) -> Option<LandmarkMaterial> {
        let (lx, lz) = self.local_column(x, z)?;
        if y >= self.base_y {
            let ly = usize::try_from(y - self.base_y).ok()?;
            return self.compiled.material_at(lx, ly, lz);
        }
        // Below the base course: foundation, only under a column that carries
        // the base course, and only above that column's own terrain.
        self.compiled.material_at(lx, 0, lz)?;
        let face = i64::from(self.ground_face(lx, lz));
        (y >= face).then_some(LandmarkMaterial::Stone)
    }

    /// The lowest and highest world `y` this landmark fills in a column.
    #[must_use]
    pub fn column(&self, x: i64, z: i64) -> Option<(i64, i64)> {
        let (lx, lz) = self.local_column(x, z)?;
        let (low, high) = self.compiled.column(lx, lz)?;
        let high = self.base_y + i64::from(high);
        let low = if low == 0 {
            i64::from(self.ground_face(lx, lz)).min(self.base_y)
        } else {
            self.base_y + i64::from(low)
        };
        Some((low, high))
    }

    /// Inclusive vertical bounds of everything this landmark writes.
    #[must_use]
    pub fn vertical_bounds(&self) -> (i64, i64) {
        let (_, size_y, _) = self.compiled.size();
        let lowest = self
            .ground
            .iter()
            .copied()
            .filter(|face| *face != i32::MAX)
            .min()
            .map_or(self.base_y, i64::from)
            .min(self.base_y);
        (
            lowest,
            self.base_y + i64::try_from(size_y).unwrap_or_default() - 1,
        )
    }

    /// The world column and height of the crown, for silhouette questions.
    #[must_use]
    pub fn crown_top(&self) -> i64 {
        self.base_y + self.descriptor.height
    }
}

/// Why a world cannot be given landmarks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    /// No column near the hint is a place a person could stand and look out.
    NoOverlook { hint: (i64, i64) },
    /// The world offers no two sites the overlook can see apart from each
    /// other.
    NoFirstPair { candidates: usize },
    /// Nothing is both hidden from the overlook and visible from a first
    /// landmark.
    NoRevealedSite { candidates: usize },
    /// A landmark would stand through the top of the region.
    AboveRegion { top: i64, ceiling: i64 },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOverlook { hint } => write!(
                formatter,
                "no discovery overlook near {hint:?}: the world offers no level, dry, open column there"
            ),
            Self::NoFirstPair { candidates } => write!(
                formatter,
                "no two of {candidates} candidate sites are visible and far enough apart from the overlook"
            ),
            Self::NoRevealedSite { candidates } => write!(
                formatter,
                "none of {candidates} candidate sites is hidden from the overlook and visible from a first landmark"
            ),
            Self::AboveRegion { top, ceiling } => {
                write!(
                    formatter,
                    "a landmark reaches {top}, above the region ceiling {ceiling}"
                )
            }
        }
    }
}

impl std::error::Error for PlanError {}

/// Every landmark in one world.
#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkPlan {
    overlook: DiscoveryOverlook,
    instances: Vec<LandmarkInstance>,
    reservations: Vec<Reservation>,
    fingerprint: u64,
}

/// How far the overlook search may wander from its hint, in columns.
const OVERLOOK_SEARCH: i64 = 48;
/// Radius, in columns, the overlook must be level and dry over.
const OVERLOOK_PAD: i64 = 4;
/// Radius, in columns, a site's pad must be level over before its footprint
/// is known.
const SITE_PAD: i64 = 11;
/// Jitter of a candidate anchor inside its lattice cell.
const SITE_JITTER: i64 = 2;
/// Height the shortest class reaches, used to judge a candidate before its
/// class is chosen. Every class is at least this tall, so the judgement is
/// conservative.
const MIN_CLASS_HEIGHT: i64 = 22;

impl LandmarkPlan {
    /// Derives the plan of a world.
    ///
    /// # Errors
    ///
    /// Returns a [`PlanError`] when the world cannot carry the composition the
    /// controls ask for. The caller is world construction, so a world without
    /// a plan fails to build rather than generating chunks with no landmarks.
    /// A world with an overlook and no landmarks.
    ///
    /// The golden world must carry the whole composition and a test says so.
    /// A diagnostic world under an arbitrary seed need not: its valley may
    /// have no pair of sites that satisfies the golden controls, and the
    /// honest answer there is a world without landmarks rather than a
    /// landmark placed where the composition does not hold. The overlook is
    /// still resolved, because it is where a viewer stands, not a landmark.
    ///
    /// # Errors
    ///
    /// [`PlanError::NoOverlook`] when the world has no level, dry clearing
    /// near the hint at all.
    pub fn bare(
        identity: &WorldIdentity,
        field: &TerrainField,
        vegetation: &VegetationSystem,
    ) -> Result<Self, PlanError> {
        let controls = identity.landmarks;
        let extent = identity.config.extent;
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let region = (
            i64::from(extent.min_chunk_x) * edge,
            (i64::from(extent.max_chunk_x) + 1) * edge - 1,
            i64::from(extent.min_chunk_z) * edge,
            (i64::from(extent.max_chunk_z) + 1) * edge - 1,
        );
        let mut occluders =
            WorldOccluders::new(field, vegetation, identity.config.tree_spacing, Vec::new());
        let overlook = resolve_overlook(&mut occluders, &controls, region)?;
        let mut plan = Self {
            overlook,
            instances: Vec::new(),
            reservations: vec![Reservation::Clearing {
                centre: overlook.column,
                radius: overlook.clearing_radius,
            }],
            fingerprint: 0,
        };
        plan.fingerprint = plan.compute_fingerprint();
        Ok(plan)
    }

    pub fn derive(
        identity: &WorldIdentity,
        field: &TerrainField,
        vegetation: &VegetationSystem,
    ) -> Result<Self, PlanError> {
        let controls = identity.landmarks;
        let extent = identity.config.extent;
        let edge = i64::from(u32::try_from(CHUNK_EDGE).unwrap_or(u32::MAX));
        let region = (
            i64::from(extent.min_chunk_x) * edge,
            (i64::from(extent.max_chunk_x) + 1) * edge - 1,
            i64::from(extent.min_chunk_z) * edge,
            (i64::from(extent.max_chunk_z) + 1) * edge - 1,
        );
        let ceiling = (i64::from(extent.max_chunk_y) + 1) * edge;

        let mut occluders =
            WorldOccluders::new(field, vegetation, identity.config.tree_spacing, Vec::new());
        let overlook = resolve_overlook(&mut occluders, &controls, region)?;

        // From here on every sight line is judged in a world whose overlook is
        // already a clearing, because that is the world the player will see.
        let mut occluders = occluders.with_reservations(vec![Reservation::Clearing {
            centre: overlook.column,
            radius: overlook.clearing_radius,
        }]);
        let observer = Observer {
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            x: overlook.column.0 as f64 + 0.5,
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            z: overlook.column.1 as f64 + 0.5,
            eye: controls.observer_eye,
        };

        let placement_stream = identity.seed.stream(StreamLabel::LandmarkPlacement);
        let shape_stream = identity.seed.stream(StreamLabel::LandmarkShape);
        let candidates = candidate_sites(
            &mut occluders,
            &controls,
            region,
            overlook.column,
            controls.first_pair_distance,
            identity.seed.raw(),
            placement_stream,
        );

        // Two visible, far apart, and against the sky.
        let mut seen: Vec<JudgedSite> = Vec::new();
        for site in &candidates {
            if !dry_line(&mut occluders, overlook.column, *site) {
                continue;
            }
            let base = f64::from(occluders.ground_face(site.0, site.1));
            let band = occluders.visible_band(
                observer,
                *site,
                base,
                base + MIN_CLASS_HEIGHT as f64,
                f64::from(u32::try_from(controls.apron).unwrap_or_default()),
                f64::from(u32::try_from(controls.max_sight_distance).unwrap_or_default()),
            );
            #[expect(clippy::cast_precision_loss, reason = "landmark heights")]
            let enough = controls.min_visible_voxels as f64;
            // Deliberately not `band.top_elevation <= max_elevation_degrees`.
            // Requiring the whole silhouette under the viewer's frame pushes
            // the first pair out to 87-96 units, where the canopy leaves 8.3
            // and 11.5 visible voxels instead of 21.3 and 22.0: a complete
            // little sliver against the sky instead of a tower rising out of
            // the top of the frame. Both were captured before choosing. What
            // this measures is how much silhouette shows *inside* the frame,
            // which is the thing a player actually reacts to.
            if band.visible >= enough
                && band.sky_backed
                && band.visible_within(controls.max_elevation_degrees) >= enough
            {
                let azimuth = azimuth_from(overlook.column, *site);
                seen.push((site.0, site.1, band, azimuth));
            }
        }
        let (first, second) = choose_pair(&seen, &controls).ok_or(PlanError::NoFirstPair {
            candidates: candidates.len(),
        })?;

        // The taller class goes where less silhouette showed.
        let (spire_site, gate_site) = if first.2.visible <= second.2.visible {
            (first, second)
        } else {
            (second, first)
        };

        let mut instances = Vec::with_capacity(3);
        instances.push(build_instance(
            &mut occluders,
            0,
            LandmarkRole::FirstChoice,
            SilhouetteClass::Spire,
            (spire_site.0, spire_site.1),
            overlook.column,
            &controls,
            shape_stream,
            identity.seed.raw(),
            spire_site.2,
            None,
            ceiling,
        )?);
        instances.push(build_instance(
            &mut occluders,
            1,
            LandmarkRole::FirstChoice,
            SilhouetteClass::Gate,
            (gate_site.0, gate_site.1),
            overlook.column,
            &controls,
            shape_stream,
            identity.seed.raw(),
            gate_site.2,
            None,
            ceiling,
        )?);

        // The third is found from one of the first two and not from the
        // overlook, which is what makes it a reveal rather than a third
        // option. It is judged in a world where the first two already stand:
        // their aprons are clearings, and a viewer stands in one of them.
        let mut occluders = occluders.with_reservations(
            std::iter::once(Reservation::Clearing {
                centre: overlook.column,
                radius: overlook.clearing_radius,
            })
            .chain(instances.iter().map(|instance| instance.reservation))
            .collect(),
        );
        let search = RevealSearch {
            controls: &controls,
            region,
            overlook: overlook.column,
            observer,
            seed: identity.seed.raw(),
            stream: placement_stream,
        };
        let (revealed_site, revealer, revealed_band) =
            choose_revealed(&mut occluders, &search, &instances)?;
        instances.push(build_instance(
            &mut occluders,
            2,
            LandmarkRole::Revealed,
            SilhouetteClass::Broken,
            revealed_site,
            instances[revealer].crown_column,
            &controls,
            shape_stream,
            identity.seed.raw(),
            revealed_band,
            Some(revealer),
            ceiling,
        )?);

        let mut reservations = vec![Reservation::Clearing {
            centre: overlook.column,
            radius: overlook.clearing_radius,
        }];
        reservations.extend(instances.iter().map(|instance| instance.reservation));

        let mut plan = Self {
            overlook,
            instances,
            reservations,
            fingerprint: 0,
        };
        plan.fingerprint = plan.compute_fingerprint();
        Ok(plan)
    }

    #[must_use]
    pub const fn overlook(&self) -> DiscoveryOverlook {
        self.overlook
    }

    #[must_use]
    pub fn instances(&self) -> &[LandmarkInstance] {
        &self.instances
    }

    #[must_use]
    pub fn reservations(&self) -> &[Reservation] {
        &self.reservations
    }

    /// Identity of the whole composition.
    #[must_use]
    pub const fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Whether a plant with this anchor and these bounds is removed.
    #[must_use]
    pub fn suppresses_plant(
        &self,
        anchor_x: i64,
        anchor_z: i64,
        min_x: i64,
        max_x: i64,
        min_z: i64,
        max_z: i64,
    ) -> bool {
        Reservation::any_suppresses(
            &self.reservations,
            anchor_x,
            anchor_z,
            min_x,
            max_x,
            min_z,
            max_z,
        )
    }

    /// The material any landmark puts at a world voxel.
    #[must_use]
    pub fn material_at(&self, x: i64, y: i64, z: i64) -> Option<LandmarkMaterial> {
        self.instances
            .iter()
            .find_map(|instance| instance.material_at(x, y, z))
    }

    /// The lowest and highest world `y` filled by landmark stone in a column.
    ///
    /// Exact solid geometry, with no notion of a body in it. The client turns
    /// this into a keep-out using the collision representation it owns.
    #[must_use]
    pub fn column(&self, x: i64, z: i64) -> Option<(i64, i64)> {
        self.instances
            .iter()
            .find_map(|instance| instance.column(x, z))
    }

    /// Every landmark that reaches into a chunk's horizontal extent.
    pub fn touching(
        &self,
        min_x: i64,
        max_x: i64,
        min_z: i64,
        max_z: i64,
    ) -> impl Iterator<Item = &LandmarkInstance> {
        self.instances
            .iter()
            .filter(move |instance| instance.touches_columns(min_x, max_x, min_z, max_z))
    }

    fn compute_fingerprint(&self) -> u64 {
        let mut bytes = Vec::with_capacity(256);
        bytes.extend_from_slice(b"veldwake.landmark.plan");
        bytes.extend_from_slice(&LANDMARK_PLAN_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.overlook.column.0.to_le_bytes());
        bytes.extend_from_slice(&self.overlook.column.1.to_le_bytes());
        bytes.extend_from_slice(&self.overlook.ground_face.to_le_bytes());
        bytes.extend_from_slice(&self.overlook.clearing_radius.to_le_bytes());
        for instance in &self.instances {
            bytes.extend_from_slice(&instance.descriptor.fingerprint().to_le_bytes());
            bytes.extend_from_slice(&instance.compiled.geometry_fingerprint().to_le_bytes());
            bytes.extend_from_slice(&instance.origin.0.to_le_bytes());
            bytes.extend_from_slice(&instance.origin.1.to_le_bytes());
            bytes.extend_from_slice(&instance.base_y.to_le_bytes());
            bytes.push(match instance.role {
                LandmarkRole::FirstChoice => 0,
                LandmarkRole::Revealed => 1,
            });
        }
        fnv1a64(&bytes)
    }
}

/// Degrees clockwise from world north, the convention every camera pose uses.
fn azimuth_from(from: (i64, i64), to: (i64, i64)) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
    let (dx, dz) = ((to.0 - from.0) as f64, (to.1 - from.1) as f64);
    let degrees = dx.atan2(-dz).to_degrees();
    if degrees < 0.0 {
        degrees + 360.0
    } else {
        degrees
    }
}

fn angle_between(a: f64, b: f64) -> f64 {
    let difference = (a - b).abs() % 360.0;
    if difference > 180.0 {
        360.0 - difference
    } else {
        difference
    }
}

fn distance_between(a: (i64, i64), b: (i64, i64)) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
    let (dx, dz) = ((a.0 - b.0) as f64, (a.1 - b.1) as f64);
    dx.hypot(dz)
}

/// Finds the column the world offers as an overlook, nearest the hint.
fn resolve_overlook(
    occluders: &mut WorldOccluders<'_>,
    controls: &LandmarkControls,
    region: (i64, i64, i64, i64),
) -> Result<DiscoveryOverlook, PlanError> {
    let hint = controls.overlook_hint;
    for radius in 0..=OVERLOOK_SEARCH {
        // A ring at a time, in a fixed order, so the nearest valid column wins
        // and ties resolve the same way in every run.
        let mut ring: BTreeSet<(i64, i64)> = BTreeSet::new();
        for offset in -radius..=radius {
            for (x, z) in [
                (hint.0 + offset, hint.1 - radius),
                (hint.0 + offset, hint.1 + radius),
                (hint.0 - radius, hint.1 + offset),
                (hint.0 + radius, hint.1 + offset),
            ] {
                ring.insert((z, x));
            }
        }
        for (z, x) in ring {
            if !level_and_dry(occluders, (x, z), OVERLOOK_PAD, region) {
                continue;
            }
            return Ok(DiscoveryOverlook {
                column: (x, z),
                ground_face: occluders.ground_face(x, z),
                clearing_radius: controls.overlook_clearing,
            });
        }
    }
    Err(PlanError::NoOverlook { hint })
}

/// Whether the straight line between two columns crosses no water.
///
/// A composition rule, not a reachability rule: the plan may not consult the
/// movement rules, and this does not pretend to. It says something simpler and
/// entirely about the world — a landmark a player is invited to walk to should
/// not have a river lying across the invitation. Whether a body can actually
/// get there is validated afterwards, by the client, with the game's own rule.
fn dry_line(occluders: &mut WorldOccluders<'_>, from: (i64, i64), to: (i64, i64)) -> bool {
    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
    let (dx, dz) = ((to.0 - from.0) as f64, (to.1 - from.1) as f64);
    let length = dx.hypot(dz);
    if length <= f64::EPSILON {
        return true;
    }
    let mut travelled = 0.0;
    while travelled <= length {
        let fraction = travelled / length;
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            reason = "region coordinates"
        )]
        let column = (
            (from.0 as f64 + dx * fraction).floor() as i64,
            (from.1 as f64 + dz * fraction).floor() as i64,
        );
        if occluders.has_water(column.0, column.1) {
            return false;
        }
        travelled += 2.0;
    }
    true
}

/// Whether a disc of columns is inside the region, dry and level to a voxel.
///
/// Staged, because this is the derivation's inner loop and almost every
/// candidate fails: five samples reject a slope or a shore, and only what
/// survives pays for the whole disc.
fn level_and_dry(
    occluders: &mut WorldOccluders<'_>,
    centre: (i64, i64),
    radius: i64,
    region: (i64, i64, i64, i64),
) -> bool {
    let margin = radius + 3;
    if centre.0 - margin < region.0
        || centre.0 + margin > region.1
        || centre.1 - margin < region.2
        || centre.1 + margin > region.3
    {
        return false;
    }
    let base = occluders.ground_face(centre.0, centre.1);
    if occluders.has_water(centre.0, centre.1) {
        return false;
    }
    for (dx, dz) in [
        (radius, 0),
        (-radius, 0),
        (0, radius),
        (0, -radius),
        (margin, margin),
        (-margin, -margin),
    ] {
        let (x, z) = (centre.0 + dx, centre.1 + dz);
        if (occluders.ground_face(x, z) - base).abs() > 1 || occluders.has_water(x, z) {
            return false;
        }
    }
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            let (x, z) = (centre.0 + dx, centre.1 + dz);
            if (occluders.ground_face(x, z) - base).abs() > 1 {
                return false;
            }
        }
    }
    // Dry, with a margin, so no landmark stands on a shoreline. Water bodies
    // are tens of columns wide, so a two-column stride cannot step over one.
    let mut dz = -margin;
    while dz <= margin {
        let mut dx = -margin;
        while dx <= margin {
            if occluders.has_water(centre.0 + dx, centre.1 + dz) {
                return false;
            }
            dx += 2;
        }
        dz += 2;
    }
    true
}

/// Candidate sites on the placement lattice, inside a distance band.
fn candidate_sites(
    occluders: &mut WorldOccluders<'_>,
    controls: &LandmarkControls,
    region: (i64, i64, i64, i64),
    centre: (i64, i64),
    band: (i64, i64),
    seed: u64,
    stream: u64,
) -> Vec<(i64, i64)> {
    let lattice = controls.site_lattice.max(1);
    let reach = band.1 + lattice;
    let mut sites = Vec::new();
    for cell_z in (centre.1 - reach).div_euclid(lattice)..=(centre.1 + reach).div_euclid(lattice) {
        for cell_x in
            (centre.0 - reach).div_euclid(lattice)..=(centre.0 + reach).div_euclid(lattice)
        {
            let hash = hash_2d(seed, stream, cell_x, cell_z);
            let jitter = |index: u64| {
                (sub_hash(hash, index) % (2 * SITE_JITTER + 1).cast_unsigned()) as i64 - SITE_JITTER
            };
            let site = (cell_x * lattice + jitter(0), cell_z * lattice + jitter(1));
            let distance = distance_between(site, centre);
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            let (low, high) = (band.0 as f64, band.1 as f64);
            if distance < low || distance > high {
                continue;
            }
            if !level_and_dry(occluders, site, SITE_PAD, region) {
                continue;
            }
            sites.push(site);
        }
    }
    sites
}

/// A candidate site with the silhouette it showed and the direction it lies
/// in: column `x`, column `z`, the band the world proxy measured, and the
/// azimuth from the overlook in degrees.
type JudgedSite = (i64, i64, VisibleBand, f64);

/// The best pair of visible sites, by the weakest silhouette of the two.
fn choose_pair<'a>(
    seen: &'a [JudgedSite],
    controls: &LandmarkControls,
) -> Option<(&'a JudgedSite, &'a JudgedSite)> {
    let mut best: Option<(f64, usize, usize)> = None;
    for (first_index, first) in seen.iter().enumerate() {
        for (second_index, second) in seen.iter().enumerate().skip(first_index + 1) {
            if angle_between(first.3, second.3) < controls.min_separation_degrees {
                continue;
            }
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            let separation = controls.min_site_separation as f64;
            if distance_between((first.0, first.1), (second.0, second.1)) < separation {
                continue;
            }
            let score = first.2.visible.min(second.2.visible);
            let better = best.is_none_or(|(current, _, _)| score > current + 1e-9);
            if better {
                best = Some((score, first_index, second_index));
            }
        }
    }
    best.map(|(_, first, second)| (&seen[first], &seen[second]))
}

/// The third site: hidden from the overlook, visible from a first landmark.
///
/// Drawn from the same candidate set as the first two. One lattice, one pad
/// check, one annulus of terrain sampled: searching a second annulus around
/// each of the first two landmarks tripled the cost of building a world and
/// bought nothing the composition needed.
/// Everything the reveal search needs that is not the world itself.
struct RevealSearch<'a> {
    controls: &'a LandmarkControls,
    region: (i64, i64, i64, i64),
    overlook: (i64, i64),
    observer: Observer,
    seed: u64,
    stream: u64,
}

fn choose_revealed(
    occluders: &mut WorldOccluders<'_>,
    search: &RevealSearch<'_>,
    placed: &[LandmarkInstance],
) -> Result<((i64, i64), usize, VisibleBand), PlanError> {
    let RevealSearch {
        controls,
        region,
        overlook,
        observer,
        seed,
        stream,
    } = *search;
    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
    let separation = controls.min_site_separation as f64;
    let apron = f64::from(u32::try_from(controls.apron).unwrap_or_default());
    let sight = f64::from(u32::try_from(controls.max_sight_distance).unwrap_or_default());
    #[expect(clippy::cast_precision_loss, reason = "landmark heights")]
    let enough = controls.min_visible_voxels as f64;
    #[expect(clippy::cast_precision_loss, reason = "landmark heights")]
    let hidden_limit = controls.max_hidden_voxels as f64;

    // The third one is searched around the landmark that will reveal it, not
    // around the overlook: what makes it a reveal is that walking to the first
    // choice is what puts it in view, and those sites lie beyond the first
    // choice, outside the ring the first pair was drawn from.
    let mut best: Option<(f64, (i64, i64), usize, VisibleBand)> = None;
    let mut examined = 0usize;
    for (revealer, instance) in placed.iter().enumerate() {
        let from = instance.crown_column;
        let eye = Observer {
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            x: from.0 as f64 + 0.5,
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            z: from.1 as f64 + 0.5,
            eye: controls.observer_eye,
        };
        let candidates = candidate_sites(
            occluders,
            controls,
            region,
            from,
            controls.reveal_distance,
            seed,
            stream ^ (revealer as u64 + 1),
        );
        examined += candidates.len();
        for site in candidates {
            if placed
                .iter()
                .any(|other| distance_between(site, other.crown_column) < separation)
                || distance_between(site, overlook) < separation
                || !dry_line(occluders, from, site)
            {
                continue;
            }
            let base = f64::from(occluders.ground_face(site.0, site.1));
            #[expect(clippy::cast_precision_loss, reason = "landmark heights")]
            let top = base + MIN_CLASS_HEIGHT as f64;

            let band = occluders.visible_band(eye, site, base, top, apron, sight);
            // The crown-in-frame rule of the first pair is deliberately not
            // applied here, and the measurement is the reason: of 111 sites
            // separated enough to be a third landmark, the forest leaves 2
            // visible from a landmark at all, and neither is far enough for
            // its crown to sit under the viewer's frame. A reveal whose top
            // is above the default pitch is a reveal; no reveal at all is
            // not. Recorded in the milestone rather than tuned away.
            if band.visible < enough
                || !band.sky_backed
                || band.visible_within(controls.max_elevation_degrees) < enough
            {
                continue;
            }

            // Hidden from the overlook: the whole point of the third one, and
            // the one test worth paying for only once the rest has passed.
            if occluders
                .visible_band(observer, site, base, top, apron, sight)
                .visible
                > hidden_limit
            {
                continue;
            }

            let better = best.is_none_or(|(current, _, _, _)| band.visible > current + 1e-9);
            if better {
                best = Some((band.visible, site, revealer, band));
            }
        }
    }
    best.map(|(_, site, revealer, band)| (site, revealer, band))
        .ok_or(PlanError::NoRevealedSite {
            candidates: examined,
        })
}

/// Compiles and places one landmark on a chosen site.
#[expect(
    clippy::too_many_arguments,
    reason = "a derivation step with no state of its own; every argument is world input"
)]
fn build_instance(
    occluders: &mut WorldOccluders<'_>,
    index: usize,
    role: LandmarkRole,
    class: SilhouetteClass,
    site: (i64, i64),
    seen_from: (i64, i64),
    controls: &LandmarkControls,
    stream: u64,
    seed: u64,
    visible: VisibleBand,
    revealed_from: Option<usize>,
    ceiling: i64,
) -> Result<LandmarkInstance, PlanError> {
    // A frame stands broadside to whoever is meant to see it, so its width is
    // what reads rather than its depth.
    let axis = if (site.0 - seen_from.0).abs() >= (site.1 - seen_from.1).abs() {
        SpanAxis::Z
    } else {
        SpanAxis::X
    };
    let descriptor = MonolithDescriptor::from_seed(
        class,
        axis,
        sub_hash(hash_2d(seed, stream, site.0, site.1), index as u64),
    );
    let compiled = CompiledMonolith::new(descriptor);
    let (size_x, size_y, size_z) = compiled.size();
    let origin = (
        site.0 - i64::try_from(size_x).unwrap_or_default() / 2,
        site.1 - i64::try_from(size_z).unwrap_or_default() / 2,
    );

    // The base course sits one voxel above the highest terrain under the
    // footprint; lower columns take a foundation.
    let mut ground = vec![i32::MAX; size_x * size_z];
    let mut base_y = i32::MIN;
    for lz in 0..size_z {
        for lx in 0..size_x {
            if compiled.column(lx, lz).is_none() {
                continue;
            }
            let x = origin.0 + i64::try_from(lx).unwrap_or_default();
            let z = origin.1 + i64::try_from(lz).unwrap_or_default();
            let face = occluders.ground_face(x, z);
            ground[lz * size_x + lx] = face;
            base_y = base_y.max(face);
        }
    }
    let base_y = i64::from(base_y);
    let top = base_y + i64::try_from(size_y).unwrap_or_default();
    if top >= ceiling {
        return Err(PlanError::AboveRegion { top, ceiling });
    }

    let bounds = (
        origin.0,
        origin.0 + i64::try_from(size_x).unwrap_or_default() - 1,
        origin.1,
        origin.1 + i64::try_from(size_z).unwrap_or_default() - 1,
    );
    let reservation = Reservation::Footprint {
        min_x: bounds.0 - controls.apron,
        max_x: bounds.1 + controls.apron,
        min_z: bounds.2 - controls.apron,
        max_z: bounds.3 + controls.apron,
    };

    Ok(LandmarkInstance {
        index,
        role,
        descriptor,
        compiled,
        origin,
        base_y,
        crown_column: site,
        bounds,
        reservation,
        revealed_from,
        visible,
        ground,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        LandmarkPlan, LandmarkRole, Reservation, angle_between, azimuth_from, distance_between,
    };
    use crate::{
        identity::{TerrainConfig, WorldIdentity, WorldSeed},
        landmark::{LANDMARK_BEHAVIOR_SIGNATURE, SilhouetteClass},
        terrain::TerrainField,
        vegetation::VegetationSystem,
    };

    /// Derives the golden plan the way the world does.
    fn golden() -> (WorldIdentity, LandmarkPlan) {
        let identity = WorldIdentity::golden();
        let field = TerrainField::new(&identity);
        let vegetation = VegetationSystem::new(&identity);
        let Ok(plan) = LandmarkPlan::derive(&identity, &field, &vegetation) else {
            panic!("the golden world carries the golden composition");
        };
        (identity, plan)
    }

    #[test]
    fn the_golden_composition_is_locked() {
        let (_, plan) = golden();
        assert_eq!(
            plan.fingerprint(),
            LANDMARK_BEHAVIOR_SIGNATURE,
            "the landmark composition moved; re-lock deliberately with an OLD/NEW/WHY paragraph"
        );
    }

    #[test]
    fn the_plan_is_a_pure_function_of_the_world() {
        let identity = WorldIdentity::golden();
        let field = TerrainField::new(&identity);
        let vegetation = VegetationSystem::new(&identity);
        let (Ok(first), Ok(second)) = (
            LandmarkPlan::derive(&identity, &field, &vegetation),
            LandmarkPlan::derive(&identity, &field, &vegetation),
        ) else {
            panic!("the golden world derives its plan");
        };
        assert_eq!(first.fingerprint(), second.fingerprint());
        for (a, b) in first.instances().iter().zip(second.instances()) {
            assert_eq!(a.origin, b.origin);
            assert_eq!(a.base_y, b.base_y);
            assert_eq!(a.crown_column, b.crown_column);
            assert_eq!(
                a.compiled.geometry_fingerprint(),
                b.compiled.geometry_fingerprint()
            );
        }
    }

    #[test]
    fn the_world_offers_three_landmarks_of_three_silhouettes() {
        let (_, plan) = golden();
        let instances = plan.instances();
        assert_eq!(instances.len(), 3);
        let mut classes: Vec<SilhouetteClass> =
            instances.iter().map(|one| one.descriptor.class).collect();
        classes.sort_unstable_by_key(|class| class.name());
        classes.dedup();
        assert_eq!(classes.len(), 3, "the three landmarks share a silhouette");
        assert_eq!(
            instances
                .iter()
                .filter(|one| one.role == LandmarkRole::FirstChoice)
                .count(),
            2,
            "a first choice is a choice between two"
        );
        let Some(revealed) = instances
            .iter()
            .find(|one| one.role == LandmarkRole::Revealed)
        else {
            panic!("one landmark is revealed by another");
        };
        let Some(revealer) = revealed.revealed_from else {
            panic!("a revealed landmark knows what reveals it");
        };
        assert_eq!(
            instances[revealer].role,
            LandmarkRole::FirstChoice,
            "a reveal comes from a first choice"
        );
    }

    #[test]
    fn the_first_choice_is_a_choice_and_not_a_direction() {
        let (identity, plan) = golden();
        let controls = identity.landmarks;
        let overlook = plan.overlook().column;
        let first: Vec<_> = plan
            .instances()
            .iter()
            .filter(|one| one.role == LandmarkRole::FirstChoice)
            .collect();
        let angle = angle_between(
            azimuth_from(overlook, first[0].crown_column),
            azimuth_from(overlook, first[1].crown_column),
        );
        assert!(
            angle >= controls.min_separation_degrees,
            "the two first choices stand {angle:.1} degrees apart, which is one direction"
        );
        for instance in first {
            let distance = distance_between(overlook, instance.crown_column);
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            let (low, high) = (
                controls.first_pair_distance.0 as f64,
                controls.first_pair_distance.1 as f64,
            );
            assert!(
                (low..=high).contains(&distance),
                "a first choice stands {distance:.0} units away, outside the composed band"
            );
            assert!(
                distance <= f64::from(u32::try_from(controls.max_sight_distance).unwrap_or(0)),
                "a first choice stands beyond the sight limit the client streams"
            );
        }
    }

    #[test]
    fn the_third_landmark_is_a_reveal_and_not_a_third_option() {
        let (identity, plan) = golden();
        let controls = identity.landmarks;
        let Some(revealed) = plan
            .instances()
            .iter()
            .find(|one| one.role == LandmarkRole::Revealed)
        else {
            panic!("a revealed landmark");
        };
        let Some(index) = revealed.revealed_from else {
            panic!("a revealer");
        };
        let revealer = plan.instances()[index].crown_column;
        let reach = distance_between(revealer, revealed.crown_column);
        #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
        let (low, high) = (
            controls.reveal_distance.0 as f64,
            controls.reveal_distance.1 as f64,
        );
        assert!(
            (low..=high).contains(&reach),
            "the reveal stands {reach:.0} units from what reveals it"
        );
        #[expect(clippy::cast_precision_loss, reason = "landmark heights")]
        let enough = controls.min_visible_voxels as f64;
        assert!(
            revealed.visible.visible >= enough,
            "the reveal shows {:.1} voxels from what reveals it",
            revealed.visible.visible
        );
    }

    #[test]
    fn no_two_landmarks_crowd_each_other() {
        // How far a landmark stands from the *overlook* is a composition band
        // per role, not this rule: a first choice is deliberately inside the
        // first-pair band, which is shorter than the separation between two
        // landmarks. What this rule forbids is two silhouettes reading as one.
        let (identity, plan) = golden();
        #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
        let separation = identity.landmarks.min_site_separation as f64;
        let overlook = plan.overlook().column;
        for (index, instance) in plan.instances().iter().enumerate() {
            if instance.role == LandmarkRole::Revealed {
                assert!(
                    distance_between(overlook, instance.crown_column) >= separation,
                    "the reveal stands next to the overlook it is meant to be hidden from"
                );
            }
            for other in plan.instances().iter().skip(index + 1) {
                assert!(
                    distance_between(instance.crown_column, other.crown_column) >= separation,
                    "landmark {index} crowds landmark {}",
                    other.index
                );
            }
        }
    }

    #[test]
    fn every_landmark_stands_inside_the_region_and_under_its_ceiling() {
        let (identity, plan) = golden();
        let extent = identity.config.extent;
        let edge = i64::from(u32::try_from(veldwake_voxel::CHUNK_EDGE).unwrap_or(u32::MAX));
        let (min_x, max_x) = (
            i64::from(extent.min_chunk_x) * edge,
            (i64::from(extent.max_chunk_x) + 1) * edge - 1,
        );
        let (min_z, max_z) = (
            i64::from(extent.min_chunk_z) * edge,
            (i64::from(extent.max_chunk_z) + 1) * edge - 1,
        );
        let ceiling = (i64::from(extent.max_chunk_y) + 1) * edge;
        for instance in plan.instances() {
            let (low_x, high_x, low_z, high_z) = instance.bounds;
            assert!(
                low_x >= min_x && high_x <= max_x && low_z >= min_z && high_z <= max_z,
                "landmark {} reaches outside the region",
                instance.index
            );
            let (base, top) = instance.vertical_bounds();
            assert!(
                base >= 0 && top < ceiling,
                "landmark {} pierces the sky",
                instance.index
            );
        }
    }

    #[test]
    fn a_reservation_covers_every_column_its_landmark_fills() {
        let (_, plan) = golden();
        for instance in plan.instances() {
            let (low_x, high_x, low_z, high_z) = instance.bounds;
            for z in [low_z, high_z] {
                for x in [low_x, high_x] {
                    // A plant anchored on a footprint corner, with the
                    // smallest possible extent, must still be removed.
                    assert!(
                        Reservation::any_suppresses(plan.reservations(), x, z, x, x, z, z),
                        "a plant may stand in landmark {} at ({x}, {z})",
                        instance.index
                    );
                }
            }
        }
    }

    #[test]
    fn a_world_the_composition_does_not_fit_has_an_overlook_and_no_landmarks() {
        // Seed 1's valley offers no site that is hidden from the overlook and
        // seen from a first choice. The honest answer is no landmarks, not a
        // landmark placed where the composition does not hold.
        let identity = WorldIdentity::new(WorldSeed(1), TerrainConfig::golden());
        let field = TerrainField::new(&identity);
        let vegetation = VegetationSystem::new(&identity);
        assert!(
            LandmarkPlan::derive(&identity, &field, &vegetation).is_err(),
            "seed 1 now carries the composition; pick another witness or widen the controls"
        );
        let Ok(bare) = LandmarkPlan::bare(&identity, &field, &vegetation) else {
            panic!("every world of this shape has an overlook");
        };
        assert!(bare.instances().is_empty());
        assert_eq!(bare.reservations().len(), 1, "only the overlook clearing");
    }
}
