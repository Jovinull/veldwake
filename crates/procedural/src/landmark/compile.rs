//! The Monolith compiler: a descriptor becomes voxels, an occupancy column
//! table and a silhouette, once.
//!
//! Everything downstream reads this one artefact. The chunk generator copies
//! its voxels, the traversal veto reads its columns, the visibility proxy
//! reads its silhouette, and a capture shows what all three agreed on. There
//! is deliberately no second description of a landmark anywhere.

use crate::hash::fnv1a64;
use crate::landmark::descriptor::{
    MonolithDescriptor, RUBBLE_APRON, SilhouetteClass, SpanAxis, draw,
};
use crate::landmark::material::LandmarkMaterial;

/// Courses of crown that take the dark band material.
const CROWN_COURSES: i64 = 2;
/// Chance in a hundred that an outer crown voxel has weathered away.
const EROSION_PERCENT: u64 = 18;
/// Chance in a hundred at a broken shaft's torn top.
const BREAK_EROSION_PERCENT: u64 = 34;

/// One landmark, compiled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledMonolith {
    descriptor: MonolithDescriptor,
    size_x: usize,
    size_y: usize,
    size_z: usize,
    voxels: Vec<Option<LandmarkMaterial>>,
    /// Lowest and highest local `y` holding a voxel, per column, or `None`.
    columns: Vec<Option<(i32, i32)>>,
    /// Build scratch: how likely each cell is to have weathered away. Emptied
    /// once erosion has run, so it is never part of a compiled landmark.
    erosion: Vec<u8>,
    geometry: u64,
}

/// What a landmark looks like from outside, in world units.
///
/// World geometry only: no field of view, no pixels, no camera. The client
/// turns this into a framing question; the plan uses it to choose a site.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Silhouette {
    /// Total height in voxels, base course included.
    pub height: i64,
    /// Footprint along x and along z.
    pub footprint: (i64, i64),
    /// Widest horizontal extent of the outline, in columns.
    pub width: i64,
    /// Columns occupied in the top quarter of the height.
    pub crown_width: i64,
    /// Height of the shorter mass over the height of the taller one.
    pub mass_ratio: f64,
    /// Columns a body can pass under: air from the ground to at least
    /// [`Silhouette::opening_clearance`].
    pub opening_columns: i64,
    /// Clear height under the opening, if there is one.
    pub opening_clearance: i64,
}

impl CompiledMonolith {
    /// Compiles a canonical descriptor.
    ///
    /// Infallible by construction: [`MonolithDescriptor::from_seed`] only
    /// produces descriptors inside the style bands, and the tests assert that
    /// every one of them compiles into a supported, asymmetric mass.
    #[must_use]
    pub fn new(descriptor: MonolithDescriptor) -> Self {
        let (span, depth) = (descriptor.span_width(), descriptor.depth);
        let lean = descriptor.lean;
        let span_cells = span + lean.abs();
        let (size_x, size_z) = match descriptor.axis {
            SpanAxis::X => (span_cells, depth),
            SpanAxis::Z => (depth, span_cells),
        };
        let size_x = usize::try_from(size_x).unwrap_or_default();
        let size_z = usize::try_from(size_z).unwrap_or_default();
        let size_y = usize::try_from(descriptor.height).unwrap_or_default();

        let mut compiled = Self {
            descriptor,
            size_x,
            size_y,
            size_z,
            voxels: vec![None; size_x * size_y * size_z],
            columns: vec![None; size_x * size_z],
            erosion: vec![0; size_x * size_y * size_z],
            geometry: 0,
        };
        match descriptor.class {
            SilhouetteClass::Spire => compiled.build_spire(),
            SilhouetteClass::Gate => compiled.build_gate(),
            SilhouetteClass::Broken => compiled.build_broken(),
        }
        compiled.weather();
        compiled.erosion = Vec::new();
        compiled.index_columns();
        compiled.geometry = compiled.compute_geometry_fingerprint();
        compiled
    }

    #[must_use]
    pub const fn descriptor(&self) -> &MonolithDescriptor {
        &self.descriptor
    }

    /// Local grid size, in voxels.
    #[must_use]
    pub const fn size(&self) -> (usize, usize, usize) {
        (self.size_x, self.size_y, self.size_z)
    }

    /// The material at a local voxel, if the landmark fills it.
    #[must_use]
    pub fn material_at(&self, x: usize, y: usize, z: usize) -> Option<LandmarkMaterial> {
        if x >= self.size_x || y >= self.size_y || z >= self.size_z {
            return None;
        }
        self.voxels[(y * self.size_z + z) * self.size_x + x]
    }

    /// Lowest and highest local `y` a column holds, if it holds anything.
    #[must_use]
    pub fn column(&self, x: usize, z: usize) -> Option<(i32, i32)> {
        if x >= self.size_x || z >= self.size_z {
            return None;
        }
        self.columns[z * self.size_x + x]
    }

    /// How many voxels the landmark is made of.
    #[must_use]
    pub fn voxel_count(&self) -> usize {
        self.voxels.iter().filter(|cell| cell.is_some()).count()
    }

    /// How many voxels carry one material.
    #[must_use]
    pub fn material_count(&self, material: LandmarkMaterial) -> usize {
        self.voxels
            .iter()
            .filter(|cell| **cell == Some(material))
            .count()
    }

    /// Identity of the compiled geometry, independent of where it stands.
    #[must_use]
    pub const fn geometry_fingerprint(&self) -> u64 {
        self.geometry
    }

    /// What the landmark looks like from outside.
    #[must_use]
    pub fn silhouette(&self) -> Silhouette {
        let height = self.descriptor.height;
        let crown_floor = height - height / 4;
        let mut crown_width = 0;
        let mut opening_columns = 0;
        let mut opening_clearance = i64::MAX;
        for z in 0..self.size_z {
            for x in 0..self.size_x {
                let Some((low, high)) = self.column(x, z) else {
                    continue;
                };
                if i64::from(high) + 1 >= crown_floor {
                    crown_width += 1;
                }
                if low > 0 {
                    // Air below the lowest voxel: something a body can walk
                    // under, which only a lintel produces.
                    opening_columns += 1;
                    opening_clearance = opening_clearance.min(i64::from(low));
                }
            }
        }
        let mut masses = self.grounded_mass_heights();
        masses.sort_unstable_by(|a, b| b.cmp(a));
        #[expect(
            clippy::cast_precision_loss,
            reason = "landmark heights are tens of voxels"
        )]
        let mass_ratio = match (masses.first(), masses.get(1)) {
            (Some(tallest), Some(second)) if *tallest > 0 => *second as f64 / *tallest as f64,
            _ => 1.0,
        };
        Silhouette {
            height,
            footprint: self.descriptor.footprint(),
            width: self.descriptor.span_width(),
            crown_width,
            mass_ratio,
            opening_columns,
            opening_clearance: if opening_columns == 0 {
                0
            } else {
                opening_clearance
            },
        }
    }

    /// Heights of every separate mass that stands on the ground.
    ///
    /// Masses, not columns: a spire tapers, so its outer columns are short
    /// while it is plainly one shape, and only a frame has two of them.
    fn grounded_mass_heights(&self) -> Vec<i64> {
        let mut grounded = vec![None; self.size_x * self.size_z];
        for z in 0..self.size_z {
            for x in 0..self.size_x {
                if let Some((0, high)) = self.column(x, z) {
                    grounded[z * self.size_x + x] = Some(i64::from(high) + 1);
                }
            }
        }
        let mut seen = vec![false; grounded.len()];
        let mut heights = Vec::new();
        for start in 0..grounded.len() {
            if seen[start] || grounded[start].is_none() {
                continue;
            }
            let mut stack = vec![start];
            seen[start] = true;
            let mut tallest = 0;
            while let Some(index) = stack.pop() {
                let Some(height) = grounded[index] else {
                    continue;
                };
                tallest = tallest.max(height);
                let (x, z) = (index % self.size_x, index / self.size_x);
                for (dx, dz) in [(1_isize, 0_isize), (-1, 0), (0, 1), (0, -1)] {
                    let (Some(nx), Some(nz)) = (x.checked_add_signed(dx), z.checked_add_signed(dz))
                    else {
                        continue;
                    };
                    if nx >= self.size_x || nz >= self.size_z {
                        continue;
                    }
                    let neighbour = nz * self.size_x + nx;
                    if !seen[neighbour] && grounded[neighbour].is_some() {
                        seen[neighbour] = true;
                        stack.push(neighbour);
                    }
                }
            }
            heights.push(tallest);
        }
        heights
    }

    // ------------------------------------------------------------- building

    fn write(&mut self, x: i64, y: i64, z: i64, material: LandmarkMaterial) {
        let (Ok(x), Ok(y), Ok(z)) = (usize::try_from(x), usize::try_from(y), usize::try_from(z))
        else {
            return;
        };
        if x >= self.size_x || y >= self.size_y || z >= self.size_z {
            return;
        }
        self.voxels[(y * self.size_z + z) * self.size_x + x] = Some(material);
    }

    /// Marks a cell as able to weather away, with a chance in a hundred.
    fn mark(&mut self, x: i64, y: i64, z: i64, percent: u8) {
        let (Ok(x), Ok(y), Ok(z)) = (usize::try_from(x), usize::try_from(y), usize::try_from(z))
        else {
            return;
        };
        if x >= self.size_x || y >= self.size_y || z >= self.size_z {
            return;
        }
        self.erosion[(y * self.size_z + z) * self.size_x + x] = percent;
    }

    /// Weathers the crown, from the top down.
    ///
    /// Top down on purpose: a stone only falls off once nothing rests on it,
    /// so erosion can never leave a voxel hanging over an empty column, and
    /// what is left is still one connected mass. The first attempt eroded in
    /// place and produced exactly that overhang.
    fn weather(&mut self) {
        let seed = self.descriptor.seed;
        for y in (1..self.size_y).rev() {
            for z in 0..self.size_z {
                for x in 0..self.size_x {
                    let index = (y * self.size_z + z) * self.size_x + x;
                    let percent = u64::from(self.erosion[index]);
                    if percent == 0 || self.voxels[index].is_none() {
                        continue;
                    }
                    // Nothing may rest on a stone that falls.
                    if y + 1 < self.size_y
                        && self.voxels[((y + 1) * self.size_z + z) * self.size_x + x].is_some()
                    {
                        continue;
                    }
                    #[expect(
                        clippy::cast_possible_wrap,
                        reason = "local landmark coordinates are tens of voxels"
                    )]
                    let key = mix(y as i64, x as i64, z as i64);
                    if draw(seed, 500 + key) % 100 < percent {
                        self.voxels[index] = None;
                    }
                }
            }
        }
    }

    /// Maps span/depth coordinates onto world-aligned local axes.
    const fn place(&self, u: i64, v: i64) -> (i64, i64) {
        match self.descriptor.axis {
            SpanAxis::X => (u, v),
            SpanAxis::Z => (v, u),
        }
    }

    /// The material of a course, before erosion.
    fn course_material(&self, y: i64, mass_top: i64) -> LandmarkMaterial {
        // The crown courses and the periodic bands are the same material for
        // the same reason — a darker course reads as a seam in the stone —
        // and the two conditions stay separate in the comment rather than in
        // the code.
        if y == 0 {
            LandmarkMaterial::Cap
        } else if y >= mass_top - CROWN_COURSES || y % self.descriptor.band_period == 0 {
            LandmarkMaterial::Band
        } else {
            LandmarkMaterial::Stone
        }
    }

    fn build_spire(&mut self) {
        let descriptor = self.descriptor;
        let half = (descriptor.shaft_width - 1) / 2;
        let point_courses = 3 + (draw(descriptor.seed, 10) % 3) as i64;
        let taper_end = (descriptor.height - point_courses).max(1);
        let base_u = if descriptor.lean < 0 { 1 } else { 0 };
        let centre_v = half;

        for y in 0..descriptor.height {
            let width = if y >= taper_end {
                0
            } else {
                half - (y * half) / taper_end
            };
            let lean = if y * 2 >= descriptor.height {
                descriptor.lean
            } else {
                0
            };
            let centre_u = base_u + half + lean;
            let material = self.course_material(y, descriptor.height);
            for dv in -width..=width {
                for du in -width..=width {
                    let (x, z) = self.place(centre_u + du, centre_v + dv);
                    self.write(x, y, z, material);
                    let outer = du.abs() == width || dv.abs() == width;
                    if outer && width > 0 && y * 3 >= descriptor.height * 2 {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "a percentage under a hundred"
                        )]
                        self.mark(x, y, z, EROSION_PERCENT as u8);
                    }
                }
            }
        }
    }

    fn build_gate(&mut self) {
        let descriptor = self.descriptor;
        let shaft = descriptor.shaft_width;
        let span = descriptor.span_width();
        let clearance = descriptor.lintel_clearance();
        let short = descriptor.short_shaft_height();

        self.build_shaft(0, shaft, descriptor.height, false);
        self.build_shaft(span - shaft, shaft, short, false);
        // The lintel is the one horizontal span the style contract allows, and
        // it is carried at both ends by construction.
        for y in clearance..clearance + descriptor.lintel_thickness {
            for u in 0..span {
                for v in 0..descriptor.depth {
                    let (x, z) = self.place(u, v);
                    self.write(x, y, z, LandmarkMaterial::Band);
                }
            }
        }
    }

    fn build_broken(&mut self) {
        let descriptor = self.descriptor;
        let shaft = descriptor.shaft_width;
        let stump = descriptor.short_shaft_height();
        let opening = descriptor.opening;

        self.build_shaft(0, shaft, descriptor.height, false);
        self.build_shaft(shaft + opening, shaft, stump, true);

        // Fallen blocks, in the opening and in the apron beyond the stump.
        let span = descriptor.span_width();
        let mut placed = 0_i64;
        let mut attempt = 0_u64;
        while placed < descriptor.rubble && attempt < 64 {
            let hash = draw(descriptor.seed, 400 + attempt);
            attempt += 1;
            let zone = shaft + (hash % (opening + RUBBLE_APRON).cast_unsigned()) as i64;
            let u = if zone < shaft + opening {
                zone
            } else {
                zone + shaft
            };
            if u >= span {
                continue;
            }
            let v = ((hash >> 16) % descriptor.depth.cast_unsigned()) as i64;
            let (x, z) = self.place(u, v);
            if self
                .material_at(
                    usize::try_from(x).unwrap_or_default(),
                    0,
                    usize::try_from(z).unwrap_or_default(),
                )
                .is_some()
            {
                continue;
            }
            let tall = i64::from((hash >> 32).is_multiple_of(2));
            for y in 0..=tall {
                self.write(x, y, z, LandmarkMaterial::Stone);
            }
            placed += 1;
        }
    }

    /// One straight shaft of the frame classes.
    fn build_shaft(&mut self, first_u: i64, width: i64, height: i64, torn: bool) {
        let descriptor = self.descriptor;
        let percent = if torn {
            BREAK_EROSION_PERCENT
        } else {
            EROSION_PERCENT
        };
        for y in 0..height {
            let material = self.course_material(y, height);
            for v in 0..descriptor.depth {
                for du in 0..width {
                    let u = first_u + du;
                    let (x, z) = self.place(u, v);
                    self.write(x, y, z, material);
                    // A crown weathers; a foot does not.
                    if y >= height - CROWN_COURSES && y > 0 {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "a percentage under a hundred"
                        )]
                        self.mark(x, y, z, percent as u8);
                    }
                }
            }
        }
    }

    fn index_columns(&mut self) {
        for z in 0..self.size_z {
            for x in 0..self.size_x {
                let mut low = None;
                let mut high = None;
                for y in 0..self.size_y {
                    if self.voxels[(y * self.size_z + z) * self.size_x + x].is_some() {
                        let y = i32::try_from(y).unwrap_or(i32::MAX);
                        low.get_or_insert(y);
                        high = Some(y);
                    }
                }
                self.columns[z * self.size_x + x] = low.zip(high);
            }
        }
    }

    fn compute_geometry_fingerprint(&self) -> u64 {
        let mut bytes = Vec::with_capacity(self.voxels.len() + 32);
        bytes.extend_from_slice(b"veldwake.landmark.geometry");
        for value in [self.size_x, self.size_y, self.size_z] {
            bytes.extend_from_slice(&u32::try_from(value).unwrap_or(u32::MAX).to_le_bytes());
        }
        for cell in &self.voxels {
            bytes.push(match cell {
                None => 0,
                Some(LandmarkMaterial::Stone) => 1,
                Some(LandmarkMaterial::Band) => 2,
                Some(LandmarkMaterial::Cap) => 3,
            });
        }
        fnv1a64(&bytes)
    }

    /// Whether every voxel is face-connected to the base course.
    #[must_use]
    pub fn is_supported(&self) -> bool {
        let cells = self.size_x * self.size_y * self.size_z;
        let mut seen = vec![false; cells];
        let mut stack = Vec::new();
        for z in 0..self.size_z {
            for x in 0..self.size_x {
                let index = z * self.size_x + x;
                if self.voxels[index].is_some() {
                    seen[index] = true;
                    stack.push((x, 0_usize, z));
                }
            }
        }
        while let Some((x, y, z)) = stack.pop() {
            let steps: [(isize, isize, isize); 6] = [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ];
            for (dx, dy, dz) in steps {
                let (Some(nx), Some(ny), Some(nz)) = (
                    x.checked_add_signed(dx),
                    y.checked_add_signed(dy),
                    z.checked_add_signed(dz),
                ) else {
                    continue;
                };
                if nx >= self.size_x || ny >= self.size_y || nz >= self.size_z {
                    continue;
                }
                let index = (ny * self.size_z + nz) * self.size_x + nx;
                if seen[index] || self.voxels[index].is_none() {
                    continue;
                }
                seen[index] = true;
                stack.push((nx, ny, nz));
            }
        }
        self.voxels
            .iter()
            .enumerate()
            .all(|(index, cell)| cell.is_none() || seen[index])
    }

    /// Whether the mass differs from its own mirror image across either axis.
    #[must_use]
    pub fn is_asymmetric(&self) -> bool {
        let mirrored_x = (0..self.size_y).any(|y| {
            (0..self.size_z).any(|z| {
                (0..self.size_x).any(|x| {
                    self.material_at(x, y, z) != self.material_at(self.size_x - 1 - x, y, z)
                })
            })
        });
        let mirrored_z = (0..self.size_y).any(|y| {
            (0..self.size_z).any(|z| {
                (0..self.size_x).any(|x| {
                    self.material_at(x, y, z) != self.material_at(x, y, self.size_z - 1 - z)
                })
            })
        });
        mirrored_x || mirrored_z
    }

    /// Whether no course of a shaft overhangs the course below it.
    ///
    /// A lintel is exempt: it is the one horizontal span the style contract
    /// allows, and it is carried at both ends.
    #[must_use]
    pub fn respects_taper(&self) -> bool {
        if self.descriptor.class == SilhouetteClass::Gate {
            return true;
        }
        for y in 1..self.size_y {
            for z in 0..self.size_z {
                for x in 0..self.size_x {
                    if self.material_at(x, y, z).is_none() {
                        continue;
                    }
                    let supported =
                        self.material_at(x, y - 1, z).is_some() || self.neighbour_below(x, y, z);
                    if !supported {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// A leaning course sits on the course below by one column of overlap.
    fn neighbour_below(&self, x: usize, y: usize, z: usize) -> bool {
        for (dx, dz) in [(1_isize, 0_isize), (-1, 0), (0, 1), (0, -1)] {
            let (Some(nx), Some(nz)) = (x.checked_add_signed(dx), z.checked_add_signed(dz)) else {
                continue;
            };
            if self.material_at(nx, y - 1, nz).is_some() {
                return true;
            }
        }
        false
    }
}

/// Mixes three small integers into one draw index.
fn mix(a: i64, b: i64, c: i64) -> u64 {
    (a.wrapping_mul(73)
        .wrapping_add(b.wrapping_mul(179))
        .wrapping_add(c.wrapping_mul(283)))
    .cast_unsigned()
        % 4096
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(class: SilhouetteClass, seed: u64) -> CompiledMonolith {
        CompiledMonolith::new(MonolithDescriptor::from_seed(class, SpanAxis::X, seed))
    }

    #[test]
    fn every_seeded_landmark_is_supported_asymmetric_and_does_not_overhang() {
        for class in SilhouetteClass::ALL {
            for axis in [SpanAxis::X, SpanAxis::Z] {
                for seed in 0..96_u64 {
                    let landmark =
                        CompiledMonolith::new(MonolithDescriptor::from_seed(class, axis, seed));
                    assert!(landmark.voxel_count() > 40, "{class:?} {seed} is tiny");
                    assert!(landmark.is_supported(), "{class:?} {seed} floats");
                    assert!(landmark.respects_taper(), "{class:?} {seed} overhangs");
                    assert!(landmark.is_asymmetric(), "{class:?} {seed} is a mirror");
                }
            }
        }
    }

    #[test]
    fn compiling_is_deterministic_and_the_geometry_fingerprint_follows_the_shape() {
        let first = compiled(SilhouetteClass::Gate, 11);
        let second = compiled(SilhouetteClass::Gate, 11);
        assert_eq!(first, second);
        assert_eq!(first.geometry_fingerprint(), second.geometry_fingerprint());
        let other = compiled(SilhouetteClass::Gate, 12);
        assert_ne!(first.geometry_fingerprint(), other.geometry_fingerprint());
        // The same descriptor on the other axis is a different arrangement of
        // the same voxels, and the fingerprint sees it.
        let turned = CompiledMonolith::new(MonolithDescriptor {
            axis: SpanAxis::Z,
            ..*first.descriptor()
        });
        assert_ne!(first.geometry_fingerprint(), turned.geometry_fingerprint());
    }

    #[test]
    fn a_gate_leaves_an_opening_a_body_can_walk_under_and_nothing_else_does() {
        for seed in 0..64_u64 {
            let gate = compiled(SilhouetteClass::Gate, seed);
            let silhouette = gate.silhouette();
            let descriptor = *gate.descriptor();
            assert_eq!(
                silhouette.opening_columns,
                descriptor.opening * descriptor.depth,
                "a gate's opening is its whole span between the shafts"
            );
            assert_eq!(silhouette.opening_clearance, descriptor.lintel_clearance());
            assert!(
                silhouette.opening_clearance >= 14,
                "a gate at seed {seed} is too low to walk under"
            );
            for class in [SilhouetteClass::Spire, SilhouetteClass::Broken] {
                assert_eq!(
                    compiled(class, seed).silhouette().opening_columns,
                    0,
                    "{class:?} should have nothing to walk under"
                );
            }
        }
    }

    #[test]
    fn the_three_classes_are_distinguishable_by_their_silhouettes() {
        for seed in 0..32_u64 {
            let spire = compiled(SilhouetteClass::Spire, seed).silhouette();
            let gate = compiled(SilhouetteClass::Gate, seed).silhouette();
            let broken = compiled(SilhouetteClass::Broken, seed).silhouette();
            // A spire is slender and whole; a gate is broad, whole and has a
            // hole under it; a broken frame is broad and is two masses of very
            // different height.
            #[expect(
                clippy::cast_precision_loss,
                reason = "landmark dimensions are tens of voxels"
            )]
            let slender = |value: &Silhouette| value.height as f64 / value.width as f64;
            assert!(slender(&spire) >= 4.0, "spire {seed}: {spire:?}");
            assert!(slender(&gate) <= 2.2, "gate {seed}: {gate:?}");
            assert!(slender(&broken) <= 2.2, "broken {seed}: {broken:?}");
            assert!(spire.opening_columns == 0 && gate.opening_columns > 0);
            assert!(gate.mass_ratio >= 0.80, "gate {seed}: {gate:?}");
            assert!(broken.mass_ratio <= 0.62, "broken {seed}: {broken:?}");
            assert!(
                spire.crown_width < gate.crown_width,
                "a spire's crown must be the narrowest"
            );
        }
    }

    #[test]
    fn a_landmark_is_banded_capped_and_mostly_stone() {
        for class in SilhouetteClass::ALL {
            for seed in 0..32_u64 {
                let landmark = compiled(class, seed);
                let total = landmark.voxel_count();
                let cap = landmark.material_count(LandmarkMaterial::Cap);
                let band = landmark.material_count(LandmarkMaterial::Band);
                assert!(cap > 0, "{class:?} {seed} has no plinth");
                assert!(band > 0, "{class:?} {seed} has no strata");
                assert!(
                    cap * 4 <= total,
                    "{class:?} {seed} is {cap} of {total} plinth, over the quarter the style allows"
                );
            }
        }
    }

    #[test]
    fn a_column_reports_the_lowest_and_highest_voxel_it_holds() {
        let gate = compiled(SilhouetteClass::Gate, 3);
        let descriptor = *gate.descriptor();
        let (size_x, size_y, size_z) = gate.size();
        for z in 0..size_z {
            for x in 0..size_x {
                let mut expected: Option<(i32, i32)> = None;
                for y in 0..size_y {
                    if gate.material_at(x, y, z).is_some() {
                        let y = i32::try_from(y).unwrap_or(i32::MAX);
                        expected = Some(expected.map_or((y, y), |(low, _)| (low, y)));
                    }
                }
                assert_eq!(gate.column(x, z), expected, "column ({x}, {z})");
            }
        }
        // The opening columns start at the lintel and nowhere lower.
        let clearance = i32::try_from(descriptor.lintel_clearance()).unwrap_or_default();
        for v in 0..descriptor.depth {
            let u = descriptor.shaft_width;
            let Some(column) = gate.column(
                usize::try_from(u).unwrap_or_default(),
                usize::try_from(v).unwrap_or_default(),
            ) else {
                panic!("a gate's opening column carries its lintel");
            };
            assert_eq!(column.0, clearance);
        }
    }
}
