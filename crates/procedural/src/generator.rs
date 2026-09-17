//! The chunk generator: the one place that turns the field, the hydrology, the
//! biome zones, and the vegetation grammar into voxels.
//!
//! Generating a chunk is a pure function of its coordinate. There is no
//! sequential state, no neighbour query, and no dependence on generation order,
//! so two chunks generated on different threads, in different runs, or years
//! apart from a disk cache agree on every voxel they share a boundary with.
//!
//! The work is batched per column rather than per voxel. Slope is a
//! neighbourhood property, so the generator evaluates the terrain field once
//! over a grid one voxel wider than the chunk on each horizontal side and
//! derives every slope from those heights, instead of evaluating the field four
//! extra times for every column.

use veldwake_voxel::{CHUNK_EDGE, Chunk, ChunkCoord, LocalCoord, VoxelId};

use crate::{
    identity::{WorldIdentity, WorldSeed},
    material::TerrainMaterial,
    terrain::{ColumnProfile, TerrainField, TerrainSample, slope_from_neighbours},
    vegetation::{MAX_SHRUB_REACH, VegetationSystem},
};

/// One voxel of halo on each horizontal side, which is what a central
/// difference over a one-voxel step needs.
const HALO: usize = 1;
/// Edge of the haloed profile grid.
const GRID_EDGE: usize = CHUNK_EDGE + 2 * HALO;

/// The generator for one world identity.
///
/// Cheap to clone and free of interior mutability, so the streaming worker can
/// hold one and the test suite can hold another without either observing the
/// other.
#[derive(Clone, Copy, Debug)]
pub struct TerrainGenerator {
    identity: WorldIdentity,
    field: TerrainField,
    vegetation: VegetationSystem,
    fingerprint: u64,
}

impl TerrainGenerator {
    #[must_use]
    pub fn new(identity: WorldIdentity) -> Self {
        Self {
            identity,
            field: TerrainField::new(&identity),
            vegetation: VegetationSystem::new(&identity),
            fingerprint: identity.fingerprint(),
        }
    }

    /// The M4 golden slice.
    #[must_use]
    pub fn golden() -> Self {
        Self::new(WorldIdentity::golden())
    }

    /// A diagnostic world: the golden configuration under another seed.
    ///
    /// Explicit seed selection, without a general configuration system. A
    /// diagnostic world is for looking at a different valley, not for tuning
    /// art controls at runtime.
    #[must_use]
    pub fn with_seed(seed: WorldSeed) -> Self {
        Self::new(WorldIdentity::new(
            seed,
            crate::identity::TerrainConfig::golden(),
        ))
    }

    #[must_use]
    pub const fn identity(&self) -> &WorldIdentity {
        &self.identity
    }

    #[must_use]
    pub const fn field(&self) -> &TerrainField {
        &self.field
    }

    #[must_use]
    pub const fn vegetation(&self) -> &VegetationSystem {
        &self.vegetation
    }

    /// Identity of everything this generator produces.
    ///
    /// A cache entry is only valid while the generator that produced it is
    /// unchanged, so this is what the disk cache stores and checks. It is a
    /// cache-invalidation key, not a save-format version, and it promises
    /// nothing about compatibility across builds.
    #[must_use]
    pub const fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Whether this coordinate is inside the generated region.
    #[must_use]
    pub const fn contains(&self, coord: ChunkCoord) -> bool {
        self.identity
            .config
            .extent
            .contains(coord.x, coord.y, coord.z)
    }

    /// Generates one chunk, or reports that the region does not reach here.
    ///
    /// `None` is authoritative absence, which the streaming runtime needs in
    /// order to distinguish "nothing here, ever" from "not loaded yet". An
    /// empty chunk inside the region is a different answer: it is sky above
    /// the terrain, and it still has neighbours that need a seam.
    #[must_use]
    pub fn generate(&self, coord: ChunkCoord) -> Option<Chunk> {
        if !self.contains(coord) {
            return None;
        }

        let mut chunk = Chunk::empty();
        let samples = self.column_samples(coord);
        self.fill_terrain(coord, &samples, &mut chunk);
        self.plant_vegetation(coord, &mut chunk);
        Some(chunk)
    }

    /// The terrain sample of every column of a chunk, in row-major order.
    ///
    /// Public because the probe binary and the fixture tests report on columns
    /// without materialising voxels.
    #[must_use]
    pub fn column_samples(&self, coord: ChunkCoord) -> Vec<TerrainSample> {
        let edge = CHUNK_EDGE as i64;
        let origin_x = i64::from(coord.x) * edge;
        let origin_z = i64::from(coord.z) * edge;

        // One profile per grid cell, halo included. Voxel centres are offset by
        // half a voxel so that a column is sampled at its middle rather than at
        // its corner, which keeps the surface symmetric about the valley axis.
        let mut profiles: Vec<ColumnProfile> = Vec::with_capacity(GRID_EDGE * GRID_EDGE);
        for row in 0..GRID_EDGE {
            let world_z = origin_z + row as i64 - HALO as i64;
            for column in 0..GRID_EDGE {
                let world_x = origin_x + column as i64 - HALO as i64;
                profiles.push(
                    self.field
                        .profile(world_x as f64 + 0.5, world_z as f64 + 0.5),
                );
            }
        }

        let mut samples = Vec::with_capacity(CHUNK_EDGE * CHUNK_EDGE);
        for local_z in 0..CHUNK_EDGE {
            for local_x in 0..CHUNK_EDGE {
                let centre = (local_z + HALO) * GRID_EDGE + local_x + HALO;
                let slope = slope_from_neighbours(
                    profiles[centre - 1].height,
                    profiles[centre + 1].height,
                    profiles[centre - GRID_EDGE].height,
                    profiles[centre + GRID_EDGE].height,
                );
                let world_x = origin_x + local_x as i64;
                let world_z = origin_z + local_z as i64;
                samples.push(self.field.sample_from(
                    world_x as f64 + 0.5,
                    world_z as f64 + 0.5,
                    profiles[centre],
                    slope,
                ));
            }
        }
        samples
    }

    /// Fills ground, strata, and standing water.
    fn fill_terrain(&self, coord: ChunkCoord, samples: &[TerrainSample], chunk: &mut Chunk) {
        let edge = CHUNK_EDGE as i64;
        let origin_y = i64::from(coord.y) * edge;

        for local_z in 0..CHUNK_EDGE {
            for local_x in 0..CHUNK_EDGE {
                let sample = &samples[local_z * CHUNK_EDGE + local_x];
                let surface_y = sample.surface_y();
                // Water fills what is below the local water surface and above
                // the ground. Because both are continuous functions of world
                // position, the waterline cannot step at a chunk seam and
                // cannot stand on a hillside.
                let water_y = sample.water_surface_y();
                if surface_y < origin_y && water_y < origin_y {
                    continue;
                }

                for local_y in 0..CHUNK_EDGE {
                    let world_y = origin_y + local_y as i64;
                    let material = if world_y <= surface_y {
                        let depth = u32::try_from(surface_y - world_y).unwrap_or(u32::MAX);
                        self.field.subsurface_material(sample, depth, world_y)
                    } else if world_y <= water_y {
                        TerrainMaterial::Water
                    } else {
                        break;
                    };
                    let previous =
                        chunk.write_local(local(local_x, local_y, local_z), material.voxel_id());
                    debug_assert!(previous.is_air(), "terrain wrote a cell twice");
                }
            }
        }
    }

    /// Writes every plant that reaches into this chunk.
    ///
    /// Vegetation only ever fills air. Terrain is already written, and terrain
    /// is a pure function of position, so the same voxel resolves the same way
    /// from whichever chunk writes it. Plants are enumerated in lattice order,
    /// which is a total order independent of the scanning window, so two plants
    /// that overlap resolve the same way in every chunk that sees them.
    fn plant_vegetation(&self, coord: ChunkCoord, chunk: &mut Chunk) {
        let edge = CHUNK_EDGE as i64;
        let origin = [
            i64::from(coord.x) * edge,
            i64::from(coord.y) * edge,
            i64::from(coord.z) * edge,
        ];
        let seed = self.vegetation.seed();

        for tree in self.vegetation.trees_touching(&self.field, coord) {
            let (low, high) = tree.bounds();
            for (y, local_y) in overlap(low[1], high[1], origin[1]) {
                for (z, local_z) in overlap(low[2], high[2], origin[2]) {
                    for (x, local_x) in overlap(low[0], high[0], origin[0]) {
                        let Some(material) = tree.material_at(seed, x, y, z) else {
                            continue;
                        };
                        write_if_air(chunk, local_x, local_y, local_z, material);
                    }
                }
            }
        }

        for shrub in self.vegetation.shrubs_touching(&self.field, coord) {
            let top = shrub.base_y + shrub.height - 1;
            for (y, local_y) in overlap(shrub.base_y, top, origin[1]) {
                for (z, local_z) in overlap(shrub.base_z, shrub.base_z + MAX_SHRUB_REACH, origin[2])
                {
                    for (x, local_x) in
                        overlap(shrub.base_x, shrub.base_x + MAX_SHRUB_REACH, origin[0])
                    {
                        let Some(material) = shrub.material_at(x, y, z) else {
                            continue;
                        };
                        write_if_air(chunk, local_x, local_y, local_z, material);
                    }
                }
            }
        }
    }
}

/// World positions in `low..=high` that fall inside a chunk starting at
/// `origin`, paired with their local index.
fn overlap(low: i64, high: i64, origin: i64) -> impl Iterator<Item = (i64, usize)> {
    let edge = CHUNK_EDGE as i64;
    let first = low.max(origin);
    let last = high.min(origin + edge - 1);
    (first..=last).map(move |world| {
        let local = usize::try_from(world - origin).unwrap_or(0);
        (world, local)
    })
}

/// The local index of a world coordinate, if it falls inside the chunk.
#[cfg(test)]
fn local_index(world: i64, origin: i64) -> Option<usize> {
    let offset = world - origin;
    if (0..CHUNK_EDGE as i64).contains(&offset) {
        usize::try_from(offset).ok()
    } else {
        None
    }
}

fn write_if_air(chunk: &mut Chunk, x: usize, y: usize, z: usize, material: TerrainMaterial) {
    let coord = local(x, y, z);
    if chunk.read_local(coord).is_air() {
        let previous = chunk.write_local(coord, material.voxel_id());
        debug_assert!(previous.is_air());
    }
}

/// Every caller here derives its indices from `0..CHUNK_EDGE`, so an invalid
/// coordinate is a bug in this module rather than bad input.
fn local(x: usize, y: usize, z: usize) -> LocalCoord {
    match LocalCoord::new(x, y, z) {
        Ok(coord) => coord,
        Err(error) => unreachable!("terrain generator built an invalid local coordinate: {error}"),
    }
}

/// Counts of each material in a chunk, for probes and fixture tests.
#[must_use]
pub fn material_histogram(chunk: &Chunk) -> Vec<(TerrainMaterial, usize)> {
    let mut counts = Vec::new();
    for material in crate::material::ALL_MATERIALS {
        let mut total = 0;
        for z in 0..CHUNK_EDGE {
            for y in 0..CHUNK_EDGE {
                for x in 0..CHUNK_EDGE {
                    if chunk.read_local(local(x, y, z)) == material.voxel_id() {
                        total += 1;
                    }
                }
            }
        }
        if total > 0 {
            counts.push((material, total));
        }
    }
    counts
}

/// Every non-air voxel identifier a chunk contains.
#[must_use]
pub fn distinct_ids(chunk: &Chunk) -> Vec<VoxelId> {
    let mut ids = Vec::new();
    for z in 0..CHUNK_EDGE {
        for y in 0..CHUNK_EDGE {
            for x in 0..CHUNK_EDGE {
                let id = chunk.read_local(local(x, y, z));
                if !id.is_air() && !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{identity::TerrainConfig, terrain::SHORE_RISE};
    use veldwake_voxel::fingerprint;

    fn generated(generator: &TerrainGenerator, coord: ChunkCoord) -> Chunk {
        match generator.generate(coord) {
            Some(chunk) => chunk,
            None => panic!("{coord:?} should be inside the golden region"),
        }
    }

    #[test]
    fn generation_is_deterministic_for_a_seed() {
        let generator = TerrainGenerator::golden();
        for coord in [
            ChunkCoord::new(0, 0, 0),
            ChunkCoord::new(-7, 1, 5),
            ChunkCoord::new(12, 2, -12),
        ] {
            let first = generated(&generator, coord);
            let second = generated(&generator, coord);
            assert_eq!(fingerprint(&first), fingerprint(&second));
            assert_eq!(first, second);
        }
    }

    #[test]
    fn generation_order_does_not_change_a_chunk() {
        let generator = TerrainGenerator::golden();
        let target = ChunkCoord::new(-2, 0, 3);
        let alone = generated(&generator, target);

        // Generate a wide neighbourhood first; the target must be untouched by
        // anything its neighbours did.
        let mut after = None;
        for z in -4..=4 {
            for x in -4..=4 {
                let coord = ChunkCoord::new(x, 0, z);
                let chunk = generated(&generator, coord);
                if coord == target {
                    after = Some(chunk);
                }
            }
        }
        assert_eq!(Some(alone), after);
    }

    #[test]
    fn a_different_seed_produces_a_different_chunk_and_fingerprint() {
        let golden = TerrainGenerator::golden();
        let other = TerrainGenerator::with_seed(WorldSeed(4_242));
        assert_ne!(golden.fingerprint(), other.fingerprint());
        let coord = ChunkCoord::new(1, 0, 1);
        assert_ne!(
            fingerprint(&generated(&golden, coord)),
            fingerprint(&generated(&other, coord))
        );
    }

    #[test]
    fn the_region_is_finite_and_absence_is_not_emptiness() {
        let generator = TerrainGenerator::golden();
        let extent = TerrainConfig::golden().extent;
        assert!(
            generator
                .generate(ChunkCoord::new(extent.max_chunk_x + 1, 0, 0))
                .is_none()
        );
        assert!(
            generator
                .generate(ChunkCoord::new(0, extent.max_chunk_y + 1, 0))
                .is_none()
        );
        assert!(generator.generate(ChunkCoord::new(0, -1, 0)).is_none());

        // A chunk of sky inside the region is present and empty, which is a
        // different answer from absent.
        let sky = generated(&generator, ChunkCoord::new(0, 2, 0));
        assert_eq!(
            sky.solid_count(),
            0,
            "the sky above the valley is not empty"
        );
    }

    #[test]
    fn a_ground_chunk_is_solid_at_the_bottom_and_open_at_the_top() {
        let generator = TerrainGenerator::golden();
        let chunk = generated(&generator, ChunkCoord::new(0, 0, 0));
        assert!(chunk.solid_count() > Chunk::VOLUME / 4);
        for z in [0, 15, 31] {
            for x in [0, 15, 31] {
                assert!(
                    !chunk.read_local(local(x, 0, z)).is_air(),
                    "a hole in the region floor at {x}, {z}"
                );
            }
        }
    }

    #[test]
    fn every_voxel_is_a_declared_terrain_material() {
        let generator = TerrainGenerator::golden();
        for coord in [
            ChunkCoord::new(0, 0, 0),
            ChunkCoord::new(3, 1, -2),
            ChunkCoord::new(-9, 0, 8),
        ] {
            let chunk = generated(&generator, coord);
            for id in distinct_ids(&chunk) {
                assert!(
                    TerrainMaterial::from_voxel_id(id).is_some(),
                    "{id:?} is not a terrain material"
                );
            }
        }
    }

    #[test]
    fn a_chunk_boundary_is_continuous_in_both_directions() {
        let generator = TerrainGenerator::golden();
        // The last column of one chunk and the first of the next describe
        // adjacent world columns, so their surface heights must differ by no
        // more than the terrain does over one voxel.
        for (left_coord, right_coord, axis) in [
            (ChunkCoord::new(-1, 0, 2), ChunkCoord::new(0, 0, 2), 0),
            (ChunkCoord::new(2, 0, -1), ChunkCoord::new(2, 0, 0), 2),
        ] {
            let left = generator.column_samples(left_coord);
            let right = generator.column_samples(right_coord);
            for index in 0..CHUNK_EDGE {
                let (last, first) = if axis == 0 {
                    (
                        left[index * CHUNK_EDGE + CHUNK_EDGE - 1].height,
                        right[index * CHUNK_EDGE].height,
                    )
                } else {
                    (
                        left[(CHUNK_EDGE - 1) * CHUNK_EDGE + index].height,
                        right[index].height,
                    )
                };
                assert!(
                    (last - first).abs() < 4.0,
                    "a {last} to {first} step across a seam"
                );
            }
        }
    }

    #[test]
    fn water_stands_level_across_a_chunk_seam() {
        let generator = TerrainGenerator::golden();
        // The two chunks either side of the pond centre.
        let left = generator.column_samples(ChunkCoord::new(2, 0, 0));
        let right = generator.column_samples(ChunkCoord::new(3, 0, 0));
        let mut submerged = 0;
        for index in 0..CHUNK_EDGE {
            let last = &left[index * CHUNK_EDGE + CHUNK_EDGE - 1];
            let first = &right[index * CHUNK_EDGE];
            if last.is_submerged() || first.is_submerged() {
                submerged += 1;
                assert!(
                    (last.water_surface - first.water_surface).abs() < 0.01,
                    "the waterline steps at the seam: {} against {}",
                    last.water_surface,
                    first.water_surface
                );
                assert_eq!(
                    last.water_surface_y(),
                    first.water_surface_y(),
                    "the top water voxel changes across the seam"
                );
            }
        }
        assert!(submerged > 0, "no water at the sampled seam");
    }

    #[test]
    fn water_never_stands_above_the_ground_it_touches() {
        let generator = TerrainGenerator::golden();
        for coord in [
            ChunkCoord::new(3, 0, 0),
            ChunkCoord::new(-5, 0, -1),
            ChunkCoord::new(0, 0, 1),
        ] {
            let chunk = generated(&generator, coord);
            let samples = generator.column_samples(coord);
            let origin_y = i64::from(coord.y) * CHUNK_EDGE as i64;
            for local_z in 0..CHUNK_EDGE {
                for local_x in 0..CHUNK_EDGE {
                    let sample = &samples[local_z * CHUNK_EDGE + local_x];
                    for local_y in 0..CHUNK_EDGE {
                        if chunk.read_local(local(local_x, local_y, local_z))
                            != TerrainMaterial::Water.voxel_id()
                        {
                            continue;
                        }
                        let world_y = origin_y + local_y as i64;
                        assert!(
                            world_y > sample.surface_y(),
                            "water inside the ground at y={world_y}"
                        );
                        assert!(
                            world_y <= sample.water_surface_y(),
                            "water above its own surface at y={world_y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn vegetation_is_grounded_and_never_stands_in_water() {
        let generator = TerrainGenerator::golden();
        let mut planted = 0;
        for coord in [
            ChunkCoord::new(0, 0, 1),
            ChunkCoord::new(-2, 0, -3),
            ChunkCoord::new(5, 0, 4),
        ] {
            let chunk = generated(&generator, coord);
            let samples = generator.column_samples(coord);
            let origin_y = i64::from(coord.y) * CHUNK_EDGE as i64;
            for local_z in 0..CHUNK_EDGE {
                for local_x in 0..CHUNK_EDGE {
                    let sample = &samples[local_z * CHUNK_EDGE + local_x];
                    for local_y in 0..CHUNK_EDGE {
                        let id = chunk.read_local(local(local_x, local_y, local_z));
                        let Some(material) = TerrainMaterial::from_voxel_id(id) else {
                            continue;
                        };
                        if !material.is_vegetation() {
                            continue;
                        }
                        planted += 1;
                        let world_y = origin_y + local_y as i64;
                        assert!(
                            world_y > sample.surface_y(),
                            "a plant buried in the ground at y={world_y}"
                        );
                        if material == TerrainMaterial::Trunk {
                            assert!(
                                sample.height > sample.water_surface + SHORE_RISE,
                                "a trunk on the water margin"
                            );
                        }
                    }
                }
            }
        }
        assert!(
            planted > 100,
            "only {planted} vegetation voxels in three chunks"
        );
    }

    #[test]
    fn a_tree_crossing_a_seam_is_written_identically_from_both_sides() {
        let generator = TerrainGenerator::golden();
        let field = generator.field();
        let vegetation = generator.vegetation();
        // Look along the valley until a tree actually straddles a boundary;
        // which chunk that happens in is a property of the world, not of the
        // rule under test.
        let mut crossings = 0;
        for column in -6..=6 {
            let left_coord = ChunkCoord::new(column, 0, 1);
            let right_coord = ChunkCoord::new(column + 1, 0, 1);
            let boundary = i64::from(column + 1) * CHUNK_EDGE as i64;
            let straddling: Vec<_> = vegetation
                .trees_touching(field, left_coord)
                .into_iter()
                .filter(|tree| {
                    let (low, high) = tree.bounds();
                    low[0] < boundary && high[0] >= boundary
                })
                .collect();
            if straddling.is_empty() {
                continue;
            }
            let left = generated(&generator, left_coord);
            let right = generated(&generator, right_coord);
            for tree in straddling {
                let (low, high) = tree.bounds();
                crossings += 1;
                for y in low[1]..=high[1] {
                    for z in low[2]..=high[2] {
                        for x in low[0]..=high[0] {
                            let Some(expected) = tree.material_at(vegetation.seed(), x, y, z)
                            else {
                                continue;
                            };
                            let (chunk, origin_x) = if x < boundary {
                                (&left, boundary - CHUNK_EDGE as i64)
                            } else {
                                (&right, boundary)
                            };
                            let (Some(lx), Some(ly), Some(lz)) = (
                                local_index(x, origin_x),
                                local_index(y, 0),
                                local_index(z, CHUNK_EDGE as i64),
                            ) else {
                                continue;
                            };
                            let actual = chunk.read_local(local(lx, ly, lz));
                            // Terrain wins over vegetation, so only assert
                            // where the cell is not ground.
                            if TerrainMaterial::from_voxel_id(actual)
                                .is_some_and(TerrainMaterial::is_vegetation)
                            {
                                assert_eq!(
                                    actual,
                                    expected.voxel_id(),
                                    "a canopy voxel differs across the seam at {x}, {y}, {z}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(crossings > 0, "no tree crosses any sampled seam");
    }

    #[test]
    fn a_ground_chunk_shows_strata_and_a_vegetated_surface() {
        let generator = TerrainGenerator::golden();
        let chunk = generated(&generator, ChunkCoord::new(0, 0, 1));
        let histogram = material_histogram(&chunk);
        let named: Vec<&str> = histogram
            .iter()
            .map(|(material, _)| material.name())
            .collect();
        for expected in ["soil", "rock", "deep-rock"] {
            assert!(named.contains(&expected), "no {expected} in {named:?}");
        }
        assert!(
            named.contains(&"meadow-grass") || named.contains(&"highland-grass"),
            "no grass in {named:?}"
        );
    }
}
