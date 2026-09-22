//! The **world** visibility proxy: can a silhouette be seen from a point in
//! the world, given only the world?
//!
//! This is placement machinery, not evidence. It knows terrain, vegetation,
//! landmark reservations, distance and geometry, and it deliberately knows
//! nothing about field of view, viewport size, fog, lighting or the camera.
//! The question it answers is "is there a conservative geometric band of
//! silhouette that the world does not hide?".
//!
//! The matching question — "with the real presentation, can a player
//! recognise it?" — is the client's, and the client answers it against real
//! captures. Math predicts here; pictures prove there.

use std::collections::HashMap;

use crate::landmark::plan::Reservation;
use crate::terrain::TerrainField;
use crate::vegetation::{MAX_TREE_REACH, VegetationSystem};

/// Where the world is looked at from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observer {
    pub x: f64,
    pub z: f64,
    /// Eye height above the ground the observer stands on.
    pub eye: f64,
}

/// What the world leaves visible of one vertical silhouette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisibleBand {
    /// Voxels of silhouette above everything the world puts in the way.
    pub visible: f64,
    /// World height of the lowest visible point.
    pub lowest_visible: f64,
    /// World height of the top.
    pub top: f64,
    /// World height of the eye it was seen from.
    pub eye: f64,
    /// Elevation, in degrees, of the lowest visible point.
    pub low_elevation: f64,
    /// Elevation, in degrees, of the top.
    pub top_elevation: f64,
    /// Horizontal distance from the observer.
    pub distance: f64,
    /// Whether the sight line to the top meets open sky beyond the silhouette.
    pub sky_backed: bool,
}

impl VisibleBand {
    /// Voxels of visible silhouette that lie within an elevation of the
    /// observer's horizontal.
    ///
    /// A composition measure, not a frame: the world has no field of view, but
    /// a silhouette that only exists far above the horizontal is one a walking
    /// person has to crane at, and the plan prefers one they can simply see.
    #[must_use]
    pub fn visible_within(&self, limit_degrees: f64) -> f64 {
        let ceiling = self.eye + limit_degrees.to_radians().tan() * self.distance;
        (self.top.min(ceiling) - self.lowest_visible).max(0.0)
    }
}

impl VisibleBand {
    /// Nothing of this silhouette is visible.
    const fn hidden(distance: f64) -> Self {
        Self {
            visible: 0.0,
            lowest_visible: 0.0,
            top: 0.0,
            eye: 0.0,
            low_elevation: 0.0,
            top_elevation: 0.0,
            distance,
            sky_backed: false,
        }
    }
}

/// How finely a sight line is sampled, in world units.
const STEP: f64 = 1.0;
/// Clearance a sight line needs over an occluder to count as clear.
const MARGIN: f64 = 0.5;

/// The world's occluders, sampled on demand and memoised for one derivation.
///
/// The memo is local to a borrow: it is not shared, not global and not visible
/// to a caller, so two derivations with the same inputs still produce the same
/// answers in the same order.
/// The horizontal extent and crown height of whatever plant a lattice cell
/// carries: `min_x`, `max_x`, `min_z`, `max_z`, and the top the plant reaches.
type PlantCell = (i64, i64, i64, i64, i32);

pub struct WorldOccluders<'a> {
    field: &'a TerrainField,
    vegetation: &'a VegetationSystem,
    spacing: i64,
    reservations: Vec<Reservation>,
    ground: HashMap<(i64, i64), i32>,
    water: HashMap<(i64, i64), bool>,
    occluder: HashMap<(i64, i64), i32>,
    cells: HashMap<(i64, i64), Option<PlantCell>>,
}

impl<'a> WorldOccluders<'a> {
    #[must_use]
    pub fn new(
        field: &'a TerrainField,
        vegetation: &'a VegetationSystem,
        spacing: i64,
        reservations: Vec<Reservation>,
    ) -> Self {
        Self {
            field,
            vegetation,
            spacing: spacing.max(1),
            reservations,
            ground: HashMap::new(),
            water: HashMap::new(),
            occluder: HashMap::new(),
            cells: HashMap::new(),
        }
    }

    /// The same world seen with another set of reservations.
    ///
    /// The terrain cache survives — ground and water are not affected by what
    /// a landmark clears — and only what vegetation does is recomputed. This
    /// is the explicit way to ask the second question of a derivation without
    /// paying for the first one twice.
    #[must_use]
    pub fn with_reservations(mut self, reservations: Vec<Reservation>) -> Self {
        self.reservations = reservations;
        self.occluder.clear();
        self.cells.clear();
        self
    }

    /// The face a sole rests on, in world voxels.
    pub fn ground_face(&mut self, x: i64, z: i64) -> i32 {
        if let Some(height) = self.ground.get(&(x, z)) {
            return *height;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "region coordinates are hundreds of voxels"
        )]
        let sample = self.field.sample(x as f64 + 0.5, z as f64 + 0.5);
        let face = i32::try_from(sample.surface_y().saturating_add(1)).unwrap_or(i32::MAX);
        self.ground.insert((x, z), face);
        face
    }

    /// Whether the generator writes a water voxel in this column.
    pub fn has_water(&mut self, x: i64, z: i64) -> bool {
        if let Some(wet) = self.water.get(&(x, z)) {
            return *wet;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "region coordinates are hundreds of voxels"
        )]
        let wet = self
            .field
            .sample(x as f64 + 0.5, z as f64 + 0.5)
            .has_water_voxel();
        self.water.insert((x, z), wet);
        wet
    }

    /// The tree anchored in one lattice cell, unless a reservation removes it.
    ///
    /// Cached as bounds and a top rather than as a descriptor: the profile only
    /// ever asks how high the canopy reaches over a column.
    fn cell(&mut self, cell_x: i64, cell_z: i64) -> Option<(i64, i64, i64, i64, i32)> {
        if let Some(entry) = self.cells.get(&(cell_x, cell_z)) {
            return *entry;
        }
        let entry = self
            .vegetation
            .tree_in_cell(self.field, cell_x, cell_z)
            .filter(|tree| {
                let (low, high) = tree.bounds();
                !Reservation::any_suppresses(
                    &self.reservations,
                    tree.base_x,
                    tree.base_z,
                    low[0],
                    high[0],
                    low[2],
                    high[2],
                )
            })
            .map(|tree| {
                let (low, high) = tree.bounds();
                (
                    low[0],
                    high[0],
                    low[2],
                    high[2],
                    i32::try_from(high[1].saturating_add(1)).unwrap_or(i32::MAX),
                )
            });
        self.cells.insert((cell_x, cell_z), entry);
        entry
    }

    /// The highest thing in a column: its ground, or the canopy over it.
    pub fn occluder(&mut self, x: i64, z: i64) -> i32 {
        if let Some(height) = self.occluder.get(&(x, z)) {
            return *height;
        }
        let mut top = self.ground_face(x, z);
        let first_x = (x - MAX_TREE_REACH).div_euclid(self.spacing);
        let last_x = (x + MAX_TREE_REACH).div_euclid(self.spacing);
        let first_z = (z - MAX_TREE_REACH).div_euclid(self.spacing);
        let last_z = (z + MAX_TREE_REACH).div_euclid(self.spacing);
        for cell_z in first_z..=last_z {
            for cell_x in first_x..=last_x {
                let Some((min_x, max_x, min_z, max_z, canopy)) = self.cell(cell_x, cell_z) else {
                    continue;
                };
                if (min_x..=max_x).contains(&x) && (min_z..=max_z).contains(&z) && canopy > top {
                    top = canopy;
                }
            }
        }
        self.occluder.insert((x, z), top);
        top
    }

    /// The eye position of an observer standing on the ground.
    pub fn eye_height(&mut self, observer: Observer) -> f64 {
        #[expect(clippy::cast_possible_truncation, reason = "a column index")]
        let (x, z) = (observer.x.floor() as i64, observer.z.floor() as i64);
        f64::from(self.ground_face(x, z)) + observer.eye
    }

    /// The least clearance a sight line has over the world, in voxels.
    ///
    /// Negative means the world is in the way. The first metre and the last
    /// `skip` units are not sampled: the observer's own column and the
    /// landmark's cleared apron are not occluders of it.
    pub fn clearance(&mut self, observer: Observer, target: (f64, f64, f64), skip: f64) -> f64 {
        let eye = self.eye_height(observer);
        let (dx, dz) = (target.0 - observer.x, target.2 - observer.z);
        let length = dx.hypot(dz);
        if length <= f64::EPSILON {
            return f64::INFINITY;
        }
        let mut worst = f64::INFINITY;
        let mut travelled = STEP;
        while travelled < length - skip {
            let fraction = travelled / length;
            let x = observer.x + dx * fraction;
            let z = observer.z + dz * fraction;
            #[expect(clippy::cast_possible_truncation, reason = "a column index")]
            let column = (x.floor() as i64, z.floor() as i64);
            let line = eye + (target.1 - eye) * fraction;
            let clearance = line - f64::from(self.occluder(column.0, column.1));
            if clearance < worst {
                worst = clearance;
            }
            travelled += STEP;
        }
        worst
    }

    /// How much of a vertical silhouette the world leaves visible.
    ///
    /// Solved rather than searched. For a sample at distance `d` along a ray
    /// of length `L`, a target at height `y` clears an occluder of height `h`
    /// exactly when `y >= eye + (h + margin - eye) * L / d`, so the lowest
    /// visible point is the largest of those bounds over the ray. One pass,
    /// and exact; the first attempt binary searched the same answer and cost
    /// nineteen passes for it.
    pub fn visible_band(
        &mut self,
        observer: Observer,
        column: (i64, i64),
        base: f64,
        top: f64,
        skip: f64,
        sight_limit: f64,
    ) -> VisibleBand {
        #[expect(
            clippy::cast_precision_loss,
            reason = "region coordinates are hundreds of voxels"
        )]
        let target = (column.0 as f64 + 0.5, column.1 as f64 + 0.5);
        let distance = (target.0 - observer.x).hypot(target.1 - observer.z);
        if distance > sight_limit || distance <= f64::EPSILON {
            return VisibleBand::hidden(distance);
        }
        let eye = self.eye_height(observer);
        let elevation = |y: f64| ((y - eye) / distance).atan().to_degrees();
        let (dx, dz) = (target.0 - observer.x, target.1 - observer.z);

        let mut lowest = base;
        let mut travelled = STEP;
        while travelled < distance - skip {
            let fraction = travelled / distance;
            let x = observer.x + dx * fraction;
            let z = observer.z + dz * fraction;
            #[expect(clippy::cast_possible_truncation, reason = "a column index")]
            let sample = (x.floor() as i64, z.floor() as i64);
            let occluder = f64::from(self.occluder(sample.0, sample.1));
            let required = eye + (occluder + MARGIN - eye) / fraction;
            if required > lowest {
                lowest = required;
            }
            travelled += STEP;
        }
        if lowest >= top {
            return VisibleBand::hidden(distance);
        }
        VisibleBand {
            visible: top - lowest,
            lowest_visible: lowest,
            top,
            eye,
            low_elevation: elevation(lowest),
            top_elevation: elevation(top),
            distance,
            sky_backed: self.sky_backed(observer, target, top, sight_limit),
        }
    }

    /// Whether the sight line to a top continues into open sky.
    ///
    /// A silhouette against sky reads; the same silhouette against a grey
    /// valley wall does not, and that is geometry rather than shading.
    fn sky_backed(
        &mut self,
        observer: Observer,
        target: (f64, f64),
        top: f64,
        sight_limit: f64,
    ) -> bool {
        let eye = self.eye_height(observer);
        let (dx, dz) = (target.0 - observer.x, target.1 - observer.z);
        let length = dx.hypot(dz);
        if length <= f64::EPSILON {
            return true;
        }
        let slope = (top - eye) / length;
        let mut travelled = length + 2.0;
        let limit = sight_limit * 2.0;
        while travelled < limit {
            let fraction = travelled / length;
            let x = observer.x + dx * fraction;
            let z = observer.z + dz * fraction;
            #[expect(clippy::cast_possible_truncation, reason = "a column index")]
            let column = (x.floor() as i64, z.floor() as i64);
            if f64::from(self.occluder(column.0, column.1)) > eye + slope * travelled {
                return false;
            }
            travelled += STEP;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Observer, WorldOccluders};
    use crate::{
        identity::WorldIdentity, landmark::plan::Reservation, terrain::TerrainField,
        vegetation::VegetationSystem,
    };

    struct World {
        field: TerrainField,
        vegetation: VegetationSystem,
        spacing: i64,
    }

    fn world() -> World {
        let identity = WorldIdentity::golden();
        World {
            field: TerrainField::new(&identity),
            vegetation: VegetationSystem::new(&identity),
            spacing: identity.config.tree_spacing,
        }
    }

    fn occluders(world: &World, reservations: Vec<Reservation>) -> WorldOccluders<'_> {
        WorldOccluders::new(&world.field, &world.vegetation, world.spacing, reservations)
    }

    /// The overlook of the golden composition, cleared of plants, which is
    /// where every sight line in the plan starts.
    fn overlook(world: &World) -> (WorldOccluders<'_>, Observer) {
        let column = (-69_i64, 49_i64);
        let mut view = occluders(
            world,
            vec![Reservation::Clearing {
                centre: column,
                radius: 40,
            }],
        );
        let ground = f64::from(view.ground_face(column.0, column.1));
        let observer = Observer {
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            x: column.0 as f64 + 0.5,
            #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
            z: column.1 as f64 + 0.5,
            eye: 3.6,
        };
        let _ = ground;
        (view, observer)
    }

    #[test]
    fn a_silhouette_behind_nothing_is_visible_to_its_base() {
        // The overlook's own clearing has no canopy in it, so a column a few
        // units away is seen from the ground up.
        let world = world();
        let (mut view, observer) = overlook(&world);
        let column = (-69, 39);
        let base = f64::from(view.ground_face(column.0, column.1));
        let band = view.visible_band(observer, column, base, base + 28.0, 0.0, 170.0);
        assert!(
            (band.visible - 28.0).abs() < 1.0,
            "a clear silhouette showed only {:.1} of its 28 voxels",
            band.visible
        );
        assert!(band.lowest_visible <= base + 1.0);
    }

    #[test]
    fn what_is_hidden_is_hidden_and_reports_no_band() {
        // A silhouette no taller than the ground it stands on cannot be seen
        // over that ground from anywhere, whatever else is in the way.
        let world = world();
        let (mut view, observer) = overlook(&world);
        let column = (-130, 62);
        let base = f64::from(view.ground_face(column.0, column.1));
        let band = view.visible_band(observer, column, base, base, 0.0, 170.0);
        assert_eq!(band.visible, 0.0);
        assert!(!band.sky_backed, "a hidden band cannot be against the sky");
    }

    #[test]
    fn a_silhouette_past_the_sight_limit_is_not_measured_at_all() {
        let world = world();
        let (mut view, observer) = overlook(&world);
        let column = (142, 71);
        let base = f64::from(view.ground_face(column.0, column.1));
        let near = view.visible_band(observer, column, base, base + 22.0, 12.0, 400.0);
        let far = view.visible_band(observer, column, base, base + 22.0, 12.0, 170.0);
        assert!(
            near.distance > 170.0,
            "the witness moved; pick another column"
        );
        assert_eq!(far.visible, 0.0, "the sight limit did not cut the line off");
    }

    #[test]
    fn the_band_under_an_elevation_limit_is_never_more_than_the_whole_band() {
        let world = world();
        let (mut view, observer) = overlook(&world);
        let column = (-130, 62);
        let base = f64::from(view.ground_face(column.0, column.1));
        let band = view.visible_band(observer, column, base, base + 28.0, 12.0, 170.0);
        assert!(band.visible > 0.0, "the golden spire's site shows nothing");
        let under_twelve = band.visible_within(12.0);
        let under_ninety = band.visible_within(90.0);
        assert!(under_twelve <= band.visible + 1e-9);
        assert!(
            (under_ninety - band.visible).abs() < 1e-9,
            "a ninety-degree limit should cut nothing"
        );
        assert!(band.visible_within(0.0) <= under_twelve);
    }

    #[test]
    fn clearing_the_plants_can_only_reveal_more_of_a_silhouette() {
        // The overlook reservation is a composition control, and this is the
        // property that makes it one: removing occluders never hides anything.
        let world = world();
        let column = (-130, 62);
        let observer = Observer {
            x: -68.5,
            z: 49.5,
            eye: 3.6,
        };
        let mut wild = occluders(&world, Vec::new());
        let base = f64::from(wild.ground_face(column.0, column.1));
        let wild_band = wild.visible_band(observer, column, base, base + 28.0, 12.0, 170.0);
        let (mut cleared, _) = overlook(&world);
        let cleared_band = cleared.visible_band(observer, column, base, base + 28.0, 12.0, 170.0);
        assert!(
            cleared_band.visible >= wild_band.visible - 1e-9,
            "clearing the overlook hid {:.1} voxels",
            wild_band.visible - cleared_band.visible
        );
    }
}
