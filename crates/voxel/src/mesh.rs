use std::{fmt, mem::size_of_val};

use crate::{CHUNK_EDGE, COARSE_EDGE, Chunk, CoarseTally, DenseGrid, GridCoord, VoxelId};

/// The six outward faces in right-handed, Y-up local chunk space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Face {
    NegativeX,
    PositiveX,
    NegativeY,
    PositiveY,
    NegativeZ,
    PositiveZ,
}

impl Face {
    pub const ALL: [Self; 6] = [
        Self::NegativeX,
        Self::PositiveX,
        Self::NegativeY,
        Self::PositiveY,
        Self::NegativeZ,
        Self::PositiveZ,
    ];

    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        match self {
            Self::NegativeX => [-1.0, 0.0, 0.0],
            Self::PositiveX => [1.0, 0.0, 0.0],
            Self::NegativeY => [0.0, -1.0, 0.0],
            Self::PositiveY => [0.0, 1.0, 0.0],
            Self::NegativeZ => [0.0, 0.0, -1.0],
            Self::PositiveZ => [0.0, 0.0, 1.0],
        }
    }

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::NegativeX => Self::PositiveX,
            Self::PositiveX => Self::NegativeX,
            Self::NegativeY => Self::PositiveY,
            Self::PositiveY => Self::NegativeY,
            Self::NegativeZ => Self::PositiveZ,
            Self::PositiveZ => Self::NegativeZ,
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// How meshing handles a required sample outside the provided neighborhood.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryPolicy {
    /// Emit the outer face of the finite chunk set.
    Expose,
    /// Reject incomplete neighborhood data rather than treating it as air.
    RequireKnown,
}

/// Result of an axial sample in a [`ChunkNeighborhood`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NeighborSample {
    Known(VoxelId),
    Missing,
}

/// Allocation-free borrowed view of one grid and its six axial neighbors.
///
/// `EDGE` defaults to the chunk edge so M2–M3B call sites are unchanged.
#[derive(Clone, Copy, Debug)]
pub struct ChunkNeighborhood<'a, const EDGE: usize = CHUNK_EDGE> {
    center: &'a DenseGrid<EDGE>,
    neighbors: [Option<&'a DenseGrid<EDGE>>; 6],
}

impl<'a, const EDGE: usize> ChunkNeighborhood<'a, EDGE> {
    #[must_use]
    pub const fn new(center: &'a DenseGrid<EDGE>) -> Self {
        Self {
            center,
            neighbors: [None; 6],
        }
    }

    #[must_use]
    pub fn with_neighbor(mut self, face: Face, chunk: &'a DenseGrid<EDGE>) -> Self {
        self.neighbors[face.index()] = Some(chunk);
        self
    }

    #[must_use]
    pub const fn center(self) -> &'a DenseGrid<EDGE> {
        self.center
    }

    /// Samples one face-adjacent cell without interpreting a missing chunk as air.
    #[must_use]
    pub fn sample_adjacent(self, coord: GridCoord<EDGE>, face: Face) -> NeighborSample {
        if let Some(local) = local_neighbor(coord, face) {
            return NeighborSample::Known(self.center.read_local(local));
        }

        let Some(neighbor) = self.neighbors[face.index()] else {
            return NeighborSample::Missing;
        };
        let (x, y, z) = (coord.x(), coord.y(), coord.z());
        let boundary = match face {
            Face::NegativeX => (EDGE - 1, y, z),
            Face::PositiveX => (0, y, z),
            Face::NegativeY => (x, EDGE - 1, z),
            Face::PositiveY => (x, 0, z),
            Face::NegativeZ => (x, y, EDGE - 1),
            Face::PositiveZ => (x, y, 0),
        };
        NeighborSample::Known(
            neighbor.read_local(local_coord_from_meshing(boundary.0, boundary.1, boundary.2)),
        )
    }
}

/// A solid boundary face required neighbor data that was not provided.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshBoundaryError<const EDGE: usize = CHUNK_EDGE> {
    pub face: Face,
    pub local: GridCoord<EDGE>,
}

impl<const EDGE: usize> fmt::Display for MeshBoundaryError<EDGE> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "missing {:?} neighbor while meshing solid voxel ({}, {}, {})",
            self.face,
            self.local.x(),
            self.local.y(),
            self.local.z()
        )
    }
}

impl<const EDGE: usize> std::error::Error for MeshBoundaryError<EDGE> {}

/// One owned, one-cell-thick boundary required by an asynchronous mesh job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaceSlab<const EDGE: usize = CHUNK_EDGE> {
    cells: Option<Box<[VoxelId]>>,
}

impl<const EDGE: usize> FaceSlab<EDGE> {
    #[must_use]
    pub const fn known_air() -> Self {
        Self { cells: None }
    }

    /// Copies only the face of `neighbor` that touches `center` in `face`.
    #[must_use]
    pub fn from_neighbor(face: Face, neighbor: &DenseGrid<EDGE>) -> Self {
        let mut cells = Vec::with_capacity(EDGE * EDGE);
        for secondary in 0..EDGE {
            for primary in 0..EDGE {
                let (x, y, z) = slab_neighbor_coord::<EDGE>(face, primary, secondary);
                cells.push(neighbor.read_local(local_coord_from_meshing(x, y, z)));
            }
        }
        Self {
            cells: Some(cells.into_boxed_slice()),
        }
    }

    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        self.cells.as_deref().map_or(0, size_of_val)
    }

    #[must_use]
    pub const fn is_known_air(&self) -> bool {
        self.cells.is_none()
    }

    fn sample(&self, face: Face, coord: GridCoord<EDGE>) -> VoxelId {
        self.cells
            .as_ref()
            .map_or(VoxelId::AIR, |cells| cells[slab_index(face, coord)])
    }
}

impl FaceSlab<COARSE_EDGE> {
    /// The coarse face of a full-resolution neighbor, derived from the two
    /// voxel layers touching `face` with the `downsample_2x` rules. Equal to
    /// `FaceSlab::from_neighbor(face, &neighbor.downsample_2x())` without
    /// deriving the whole grid.
    #[must_use]
    pub fn downsampled_from(face: Face, neighbor: &Chunk) -> Self {
        let mut cells = Vec::with_capacity(COARSE_EDGE * COARSE_EDGE);
        for secondary in 0..COARSE_EDGE {
            for primary in 0..COARSE_EDGE {
                cells.push(coarse_face_material(face, neighbor, primary, secondary));
            }
        }
        Self {
            cells: Some(cells.into_boxed_slice()),
        }
    }

    /// Coverage mask a coarse center sees across a seam to a neighbor
    /// presented at fine resolution. A coarse cell is marked covered (solid,
    /// carrying the majority material) only when all four fine cells of the
    /// neighbor's touching layer are solid; then the coarse seam face is
    /// suppressed because the fine geometry closes it completely. Partial
    /// coverage leaves the cell AIR so the whole coarse quad stays, behind the
    /// fine solids where they exist; the quad is never subdivided.
    #[must_use]
    pub fn fine_coverage_of(face: Face, neighbor: &Chunk) -> Self {
        let mut cells = Vec::with_capacity(COARSE_EDGE * COARSE_EDGE);
        for secondary in 0..COARSE_EDGE {
            for primary in 0..COARSE_EDGE {
                let mut tally = CoarseTally::new();
                for ds in 0..2 {
                    for dp in 0..2 {
                        let (x, y, z) =
                            fine_layer_coord(face, 0, 2 * primary + dp, 2 * secondary + ds);
                        tally.add(neighbor.read_local(local_coord_from_meshing(x, y, z)));
                    }
                }
                cells.push(if tally.solid_count() == 4 {
                    tally.material()
                } else {
                    VoxelId::AIR
                });
            }
        }
        Self {
            cells: Some(cells.into_boxed_slice()),
        }
    }
}

impl FaceSlab<CHUNK_EDGE> {
    /// The occupancy a fine chunk sees across a seam to a neighbor presented
    /// at coarse resolution: every fine seam cell samples the coarse block
    /// covering it, so a fine face is emitted only where that block is AIR.
    #[must_use]
    pub fn coarse_occupancy_of(face: Face, neighbor: &Chunk) -> Self {
        let mut coarse = Vec::with_capacity(COARSE_EDGE * COARSE_EDGE);
        for secondary in 0..COARSE_EDGE {
            for primary in 0..COARSE_EDGE {
                coarse.push(coarse_face_material(face, neighbor, primary, secondary));
            }
        }
        let mut cells = Vec::with_capacity(CHUNK_EDGE * CHUNK_EDGE);
        for secondary in 0..CHUNK_EDGE {
            for primary in 0..CHUNK_EDGE {
                cells.push(coarse[primary / 2 + COARSE_EDGE * (secondary / 2)]);
            }
        }
        Self {
            cells: Some(cells.into_boxed_slice()),
        }
    }
}

/// Fine coordinate in a neighbor's layer `depth` (0 touches the center) for
/// the seam in `face`, matching `from_neighbor`'s orientation convention.
fn fine_layer_coord(face: Face, depth: usize, p: usize, sec: usize) -> (usize, usize, usize) {
    match face {
        Face::NegativeX => (CHUNK_EDGE - 1 - depth, p, sec),
        Face::PositiveX => (depth, p, sec),
        Face::NegativeY => (p, CHUNK_EDGE - 1 - depth, sec),
        Face::PositiveY => (p, depth, sec),
        Face::NegativeZ => (p, sec, CHUNK_EDGE - 1 - depth),
        Face::PositiveZ => (p, sec, depth),
    }
}

/// Material of the coarse cell `(primary, secondary)` on `face` of `neighbor`,
/// computed from its 2×2×2 fine block with the shared [`CoarseTally`] rule.
fn coarse_face_material(face: Face, neighbor: &Chunk, primary: usize, secondary: usize) -> VoxelId {
    let mut tally = CoarseTally::new();
    for depth in 0..2 {
        for ds in 0..2 {
            for dp in 0..2 {
                let (x, y, z) = fine_layer_coord(face, depth, 2 * primary + dp, 2 * secondary + ds);
                tally.add(neighbor.read_local(local_coord_from_meshing(x, y, z)));
            }
        }
    }
    tally.material()
}

/// Self-contained input for detached exposed-face meshing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedMeshingSnapshot<const EDGE: usize = CHUNK_EDGE> {
    center: DenseGrid<EDGE>,
    faces: [FaceSlab<EDGE>; 6],
}

impl<const EDGE: usize> OwnedMeshingSnapshot<EDGE> {
    #[must_use]
    pub const fn new(center: DenseGrid<EDGE>, faces: [FaceSlab<EDGE>; 6]) -> Self {
        Self { center, faces }
    }

    #[must_use]
    pub const fn center(&self) -> &DenseGrid<EDGE> {
        &self.center
    }

    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        DenseGrid::<EDGE>::BYTES
            + self
                .faces
                .iter()
                .map(FaceSlab::payload_bytes)
                .sum::<usize>()
    }

    fn sample_adjacent(&self, coord: GridCoord<EDGE>, face: Face) -> NeighborSample {
        if let Some(local) = local_neighbor(coord, face) {
            return NeighborSample::Known(self.center.read_local(local));
        }
        NeighborSample::Known(self.faces[face.index()].sample(face, coord))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub voxel: VoxelId,
    pub face: Face,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl Mesh {
    #[must_use]
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    #[must_use]
    pub fn quad_count(&self) -> usize {
        self.indices.len() / 6
    }

    /// Logical payload bytes; excludes `Vec` capacity and allocator overhead.
    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        size_of_val(self.vertices.as_slice()) + size_of_val(self.indices.as_slice())
    }
}

/// Builds the correctness-reference mesh by emitting each face exposed to air.
///
/// This M2-compatible convenience API explicitly exposes every missing outer
/// neighbor. Use [`mesh_exposed_faces_with_neighbors`] when seams matter.
/// Vertices for each quad are counter-clockwise when viewed from outside.
#[must_use]
pub fn mesh_exposed_faces<const EDGE: usize>(chunk: &DenseGrid<EDGE>) -> Mesh {
    match mesh_exposed_faces_with_neighbors(ChunkNeighborhood::new(chunk), BoundaryPolicy::Expose) {
        Ok(mesh) => mesh,
        Err(error) => unreachable!("Expose boundary policy unexpectedly failed: {error}"),
    }
}

/// Builds an exposed-face mesh from an explicit local neighborhood.
pub fn mesh_exposed_faces_with_neighbors<const EDGE: usize>(
    neighborhood: ChunkNeighborhood<'_, EDGE>,
    boundary_policy: BoundaryPolicy,
) -> Result<Mesh, MeshBoundaryError<EDGE>> {
    mesh_with_sampler(neighborhood.center(), boundary_policy, |coord, face| {
        neighborhood.sample_adjacent(coord, face)
    })
}

/// Meshes a self-contained center-and-six-slabs snapshot.
#[must_use]
pub fn mesh_exposed_faces_from_snapshot<const EDGE: usize>(
    snapshot: &OwnedMeshingSnapshot<EDGE>,
) -> Mesh {
    match mesh_with_sampler(
        snapshot.center(),
        BoundaryPolicy::RequireKnown,
        |coord, face| snapshot.sample_adjacent(coord, face),
    ) {
        Ok(mesh) => mesh,
        Err(error) => unreachable!("owned snapshot unexpectedly lacked a face: {error}"),
    }
}

/// The single exposed-face loop shared by every edge; positions are cell
/// units of the meshed grid, so a coarse grid scales at presentation time.
fn mesh_with_sampler<const EDGE: usize>(
    center: &DenseGrid<EDGE>,
    boundary_policy: BoundaryPolicy,
    mut sample_adjacent: impl FnMut(GridCoord<EDGE>, Face) -> NeighborSample,
) -> Result<Mesh, MeshBoundaryError<EDGE>> {
    let mut mesh = Mesh::default();

    for z in 0..EDGE {
        for y in 0..EDGE {
            for x in 0..EDGE {
                let coord = local_coord_from_meshing(x, y, z);
                let voxel = center.read_local(coord);
                if voxel.is_air() {
                    continue;
                }

                for face in Face::ALL {
                    match sample_adjacent(coord, face) {
                        NeighborSample::Known(neighbor) if neighbor.is_air() => {
                            emit_quad(&mut mesh, x, y, z, face, voxel);
                        }
                        NeighborSample::Known(_) => {}
                        NeighborSample::Missing if boundary_policy == BoundaryPolicy::Expose => {
                            emit_quad(&mut mesh, x, y, z, face, voxel);
                        }
                        NeighborSample::Missing => {
                            return Err(MeshBoundaryError { face, local: coord });
                        }
                    }
                }
            }
        }
    }

    Ok(mesh)
}

fn local_neighbor<const EDGE: usize>(
    coord: GridCoord<EDGE>,
    face: Face,
) -> Option<GridCoord<EDGE>> {
    let (x, y, z) = (coord.x(), coord.y(), coord.z());
    let value = match face {
        Face::NegativeX => x.checked_sub(1).map(|next| (next, y, z)),
        Face::PositiveX => (x + 1 < EDGE).then_some((x + 1, y, z)),
        Face::NegativeY => y.checked_sub(1).map(|next| (x, next, z)),
        Face::PositiveY => (y + 1 < EDGE).then_some((x, y + 1, z)),
        Face::NegativeZ => z.checked_sub(1).map(|next| (x, y, next)),
        Face::PositiveZ => (z + 1 < EDGE).then_some((x, y, z + 1)),
    }?;
    Some(local_coord_from_meshing(value.0, value.1, value.2))
}

fn slab_neighbor_coord<const EDGE: usize>(
    face: Face,
    primary: usize,
    secondary: usize,
) -> (usize, usize, usize) {
    match face {
        Face::NegativeX => (EDGE - 1, primary, secondary),
        Face::PositiveX => (0, primary, secondary),
        Face::NegativeY => (primary, EDGE - 1, secondary),
        Face::PositiveY => (primary, 0, secondary),
        Face::NegativeZ => (primary, secondary, EDGE - 1),
        Face::PositiveZ => (primary, secondary, 0),
    }
}

fn slab_index<const EDGE: usize>(face: Face, coord: GridCoord<EDGE>) -> usize {
    match face {
        Face::NegativeX | Face::PositiveX => coord.y() + EDGE * coord.z(),
        Face::NegativeY | Face::PositiveY => coord.x() + EDGE * coord.z(),
        Face::NegativeZ | Face::PositiveZ => coord.x() + EDGE * coord.y(),
    }
}

fn local_coord_from_meshing<const EDGE: usize>(x: usize, y: usize, z: usize) -> GridCoord<EDGE> {
    match GridCoord::new(x, y, z) {
        Ok(coord) => coord,
        Err(error) => unreachable!("mesher loop generated invalid coordinate: {error}"),
    }
}

fn emit_quad(mesh: &mut Mesh, x: usize, y: usize, z: usize, face: Face, voxel: VoxelId) {
    let base = match u32::try_from(mesh.vertices.len()) {
        Ok(base) => base,
        Err(_) => unreachable!("chunk mesh exceeds u32 vertex addressing"),
    };
    let normal = face.normal();
    let [x0, y0, z0] = [x as f32, y as f32, z as f32];
    let [x1, y1, z1] = [x0 + 1.0, y0 + 1.0, z0 + 1.0];
    let positions = match face {
        Face::NegativeX => [[x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [x0, y0, z0]],
        Face::PositiveX => [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
        Face::NegativeY => [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
        Face::PositiveY => [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
        Face::NegativeZ => [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
        Face::PositiveZ => [[x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [x0, y0, z1]],
    };

    mesh.vertices.extend(positions.map(|position| Vertex {
        position,
        normal,
        voxel,
        face,
    }));
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{COARSE_EDGE, Chunk, ChunkBoundsError, CoarseGrid, LocalCoord};

    fn chunk_with(cells: &[(usize, usize, usize, VoxelId)]) -> Result<Chunk, ChunkBoundsError> {
        let mut chunk = Chunk::empty();
        for &(x, y, z, voxel) in cells {
            chunk.write(x, y, z, voxel)?;
        }
        Ok(chunk)
    }

    #[test]
    fn empty_chunk_emits_no_geometry() {
        let mesh = mesh_exposed_faces(&Chunk::empty());
        assert_eq!(mesh.quad_count(), 0);
        assert!(mesh.vertices().is_empty());
        assert!(mesh.indices().is_empty());
    }

    #[test]
    fn single_voxel_emits_six_quads_and_every_direction() -> Result<(), ChunkBoundsError> {
        let chunk = chunk_with(&[(4, 5, 6, VoxelId(3))])?;
        let mesh = mesh_exposed_faces(&chunk);
        assert_eq!(mesh.quad_count(), 6);
        assert_eq!(mesh.vertices().len(), 24);
        assert_eq!(mesh.indices().len(), 36);
        for face in Face::ALL {
            assert_eq!(
                mesh.vertices()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|quad| quad[0].face == face)
                    .count(),
                1
            );
            assert!(
                mesh.vertices()
                    .iter()
                    .filter(|vertex| vertex.face == face)
                    .all(|vertex| vertex.normal == face.normal())
            );
        }
        Ok(())
    }

    #[test]
    fn adjacent_voxels_remove_both_sides_of_internal_face() -> Result<(), ChunkBoundsError> {
        let chunk = chunk_with(&[(5, 5, 5, VoxelId(1)), (6, 5, 5, VoxelId(2))])?;
        let mesh = mesh_exposed_faces(&chunk);
        assert_eq!(mesh.quad_count(), 10);
        assert_eq!(mesh.vertices().len(), 40);
        assert_eq!(mesh.indices().len(), 60);
        assert!(!mesh.vertices().as_chunks::<4>().0.iter().any(|quad| {
            let vertex = quad[0];
            (vertex.voxel == VoxelId(1) && vertex.face == Face::PositiveX)
                || (vertex.voxel == VoxelId(2) && vertex.face == Face::NegativeX)
        }));
        Ok(())
    }

    #[test]
    fn solid_chunk_emits_only_six_outer_surfaces() -> Result<(), ChunkBoundsError> {
        let mut chunk = Chunk::empty();
        for z in 0..CHUNK_EDGE {
            for y in 0..CHUNK_EDGE {
                for x in 0..CHUNK_EDGE {
                    chunk.write(x, y, z, VoxelId(1))?;
                }
            }
        }
        let mesh = mesh_exposed_faces(&chunk);
        assert_eq!(mesh.quad_count(), 6 * CHUNK_EDGE * CHUNK_EDGE);
        assert_eq!(mesh.quad_count(), 6_144);
        Ok(())
    }

    #[test]
    fn triangle_winding_matches_each_outward_normal() -> Result<(), ChunkBoundsError> {
        let chunk = chunk_with(&[(3, 4, 5, VoxelId(1))])?;
        let mesh = mesh_exposed_faces(&chunk);

        for triangle in mesh.indices().as_chunks::<3>().0 {
            let a = mesh.vertices()[triangle[0] as usize];
            let b = mesh.vertices()[triangle[1] as usize];
            let c = mesh.vertices()[triangle[2] as usize];
            let cross = cross(
                subtract(b.position, a.position),
                subtract(c.position, a.position),
            );
            assert!(dot(cross, a.normal) > 0.0, "wrong winding for {:?}", a.face);
        }
        Ok(())
    }

    #[test]
    fn solid_seams_are_removed_in_all_six_directions() -> Result<(), Box<dyn std::error::Error>> {
        for face in Face::ALL {
            let center_coord = boundary_coord(face)?;
            let neighbor_coord = boundary_coord(face.opposite())?;
            let center = chunk_with(&[(
                center_coord.x(),
                center_coord.y(),
                center_coord.z(),
                VoxelId(1),
            )])?;
            let neighbor = chunk_with(&[(
                neighbor_coord.x(),
                neighbor_coord.y(),
                neighbor_coord.z(),
                VoxelId(7),
            )])?;

            let center_mesh = mesh_exposed_faces_with_neighbors(
                ChunkNeighborhood::new(&center).with_neighbor(face, &neighbor),
                BoundaryPolicy::RequireKnown,
            )?;
            let neighbor_mesh = mesh_exposed_faces_with_neighbors(
                ChunkNeighborhood::new(&neighbor).with_neighbor(face.opposite(), &center),
                BoundaryPolicy::RequireKnown,
            )?;

            assert_eq!(center_mesh.quad_count(), 5, "center seam at {face:?}");
            assert_eq!(neighbor_mesh.quad_count(), 5, "neighbor seam at {face:?}");
            assert!(
                !center_mesh
                    .vertices()
                    .iter()
                    .any(|vertex| vertex.face == face)
            );
            assert!(
                !neighbor_mesh
                    .vertices()
                    .iter()
                    .any(|vertex| vertex.face == face.opposite())
            );
        }
        Ok(())
    }

    #[test]
    fn known_air_across_a_boundary_keeps_the_face() -> Result<(), Box<dyn std::error::Error>> {
        let center = chunk_with(&[(31, 10, 11, VoxelId(2))])?;
        let neighbor = Chunk::empty();
        let mesh = mesh_exposed_faces_with_neighbors(
            ChunkNeighborhood::new(&center).with_neighbor(Face::PositiveX, &neighbor),
            BoundaryPolicy::RequireKnown,
        )?;

        assert_eq!(mesh.quad_count(), 6);
        assert!(
            mesh.vertices()
                .iter()
                .any(|vertex| vertex.face == Face::PositiveX)
        );
        Ok(())
    }

    #[test]
    fn missing_neighbor_policy_is_explicit() -> Result<(), Box<dyn std::error::Error>> {
        let center = chunk_with(&[(31, 10, 11, VoxelId(2))])?;
        let neighborhood = ChunkNeighborhood::new(&center);
        let local = LocalCoord::new(31, 10, 11)?;

        assert_eq!(
            neighborhood.sample_adjacent(local, Face::PositiveX),
            NeighborSample::Missing
        );
        assert_eq!(
            mesh_exposed_faces_with_neighbors(neighborhood, BoundaryPolicy::Expose)?.quad_count(),
            6
        );
        assert_eq!(
            mesh_exposed_faces_with_neighbors(neighborhood, BoundaryPolicy::RequireKnown),
            Err(MeshBoundaryError {
                face: Face::PositiveX,
                local,
            })
        );
        Ok(())
    }

    #[test]
    fn owned_face_slabs_sample_the_touching_cell_in_all_directions()
    -> Result<(), Box<dyn std::error::Error>> {
        for face in Face::ALL {
            let center_local = boundary_coord(face)?;
            let neighbor_local = boundary_coord(face.opposite())?;
            let center = chunk_with(&[(
                center_local.x(),
                center_local.y(),
                center_local.z(),
                VoxelId(3),
            )])?;
            let neighbor = chunk_with(&[(
                neighbor_local.x(),
                neighbor_local.y(),
                neighbor_local.z(),
                VoxelId(9),
            )])?;
            let faces = std::array::from_fn(|index| {
                let candidate = Face::ALL[index];
                if candidate == face {
                    FaceSlab::from_neighbor(candidate, &neighbor)
                } else {
                    FaceSlab::known_air()
                }
            });
            let mesh = mesh_exposed_faces_from_snapshot(&OwnedMeshingSnapshot::new(center, faces));

            assert_eq!(mesh.quad_count(), 5, "owned slab at {face:?}");
            assert!(!mesh.vertices().iter().any(|vertex| vertex.face == face));
        }
        Ok(())
    }

    #[test]
    fn borrowed_neighborhood_and_owned_snapshot_have_identical_topology()
    -> Result<(), Box<dyn std::error::Error>> {
        let center = crate::diagnostic_fixture();
        let mut neighbors: [Chunk; 6] = std::array::from_fn(|_| Chunk::empty());
        for (index, face) in Face::ALL.into_iter().enumerate() {
            let local = boundary_coord(face.opposite())?;
            assert_eq!(
                neighbors[index].write_local(local, VoxelId((index + 11) as u16)),
                VoxelId::AIR
            );
        }

        let mut borrowed = ChunkNeighborhood::new(&center);
        for (index, face) in Face::ALL.into_iter().enumerate() {
            borrowed = borrowed.with_neighbor(face, &neighbors[index]);
        }
        let borrowed_mesh =
            mesh_exposed_faces_with_neighbors(borrowed, BoundaryPolicy::RequireKnown)?;
        let slabs = std::array::from_fn(|index| {
            FaceSlab::from_neighbor(Face::ALL[index], &neighbors[index])
        });
        let snapshot = OwnedMeshingSnapshot::new(center, slabs);
        let owned_mesh = mesh_exposed_faces_from_snapshot(&snapshot);

        assert_eq!(snapshot.payload_bytes(), crate::CHUNK_BYTES + 6 * 2_048);
        assert_eq!(owned_mesh, borrowed_mesh);
        Ok(())
    }

    #[test]
    fn coarse_grid_meshes_with_the_same_loop() -> Result<(), ChunkBoundsError> {
        let mut grid = CoarseGrid::empty();
        grid.write(4, 5, 6, VoxelId(3))?;
        let mesh = mesh_exposed_faces(&grid);
        assert_eq!(mesh.quad_count(), 6);
        assert!(mesh.vertices().iter().all(|vertex| {
            vertex
                .position
                .iter()
                .all(|axis| (0.0..=16.0).contains(axis))
        }));
        let mut full = CoarseGrid::empty();
        for z in 0..COARSE_EDGE {
            for y in 0..COARSE_EDGE {
                for x in 0..COARSE_EDGE {
                    full.write(x, y, z, VoxelId(1))?;
                }
            }
        }
        assert_eq!(mesh_exposed_faces(&full).quad_count(), 6 * 16 * 16);
        Ok(())
    }

    #[test]
    fn coarse_slabs_and_seams_work_in_all_six_directions() -> Result<(), Box<dyn std::error::Error>>
    {
        for face in Face::ALL {
            let center_local = boundary_coord_of::<COARSE_EDGE>(face)?;
            let neighbor_local = boundary_coord_of::<COARSE_EDGE>(face.opposite())?;
            let mut center = CoarseGrid::empty();
            assert_eq!(center.write_local(center_local, VoxelId(3)), VoxelId::AIR);
            let mut neighbor = CoarseGrid::empty();
            assert_eq!(
                neighbor.write_local(neighbor_local, VoxelId(9)),
                VoxelId::AIR
            );

            let borrowed = mesh_exposed_faces_with_neighbors(
                ChunkNeighborhood::new(&center).with_neighbor(face, &neighbor),
                BoundaryPolicy::RequireKnown,
            )?;
            let faces: [FaceSlab<COARSE_EDGE>; 6] = std::array::from_fn(|index| {
                let candidate = Face::ALL[index];
                if candidate == face {
                    FaceSlab::from_neighbor(candidate, &neighbor)
                } else {
                    FaceSlab::known_air()
                }
            });
            let snapshot = OwnedMeshingSnapshot::new(center, faces);
            assert_eq!(snapshot.payload_bytes(), 8_192 + 512);
            let owned = mesh_exposed_faces_from_snapshot(&snapshot);

            assert_eq!(borrowed.quad_count(), 5, "coarse seam at {face:?}");
            assert_eq!(owned, borrowed);
            assert!(!owned.vertices().iter().any(|vertex| vertex.face == face));
        }
        Ok(())
    }

    #[test]
    fn missing_coarse_neighbor_is_never_air() -> Result<(), Box<dyn std::error::Error>> {
        let mut center = CoarseGrid::empty();
        center.write(15, 3, 4, VoxelId(2))?;
        let neighborhood = ChunkNeighborhood::new(&center);
        assert_eq!(
            mesh_exposed_faces_with_neighbors(neighborhood, BoundaryPolicy::RequireKnown),
            Err(MeshBoundaryError {
                face: Face::PositiveX,
                local: GridCoord::<COARSE_EDGE>::new(15, 3, 4)?,
            })
        );
        Ok(())
    }

    /// Seam-plane oracle for one fine chunk next to a coarse-presented chunk.
    /// Returns (fine faces expected, coarse faces expected).
    fn mixed_seam_expectation(face: Face, fine: &Chunk, coarse_source: &Chunk) -> (usize, usize) {
        let mut fine_faces = 0;
        let mut coarse_faces = 0;
        let last = CHUNK_EDGE - 1;
        for secondary in 0..CHUNK_EDGE {
            for primary in 0..CHUNK_EDGE {
                let (x, y, z) = match face {
                    Face::NegativeX => (0, primary, secondary),
                    Face::PositiveX => (last, primary, secondary),
                    Face::NegativeY => (primary, 0, secondary),
                    Face::PositiveY => (primary, last, secondary),
                    Face::NegativeZ => (primary, secondary, 0),
                    Face::PositiveZ => (primary, secondary, last),
                };
                let fine_solid = !fine.read(x, y, z).is_ok_and(|voxel| voxel.is_air());
                // `coarse_face_material` already selects the neighbor layers
                // that touch the center in `face`, exactly like `from_neighbor`.
                let block_solid =
                    !coarse_face_material(face, coarse_source, primary / 2, secondary / 2).is_air();
                if fine_solid && !block_solid {
                    fine_faces += 1;
                }
                if block_solid && primary % 2 == 0 && secondary % 2 == 0 {
                    // Suppressed only when the fine seam layer fully covers it.
                    let covered = (0..4).all(|bit| {
                        let (x, y, z) = fine_layer_coord(
                            face.opposite(),
                            0,
                            primary + (bit & 1),
                            secondary + (bit >> 1),
                        );
                        fine.read(x, y, z).is_ok_and(|voxel| !voxel.is_air())
                    });
                    if !covered {
                        coarse_faces += 1;
                    }
                }
            }
        }
        (fine_faces, coarse_faces)
    }

    fn mixed_seam_meshes(face: Face, fine: &Chunk, coarse_source: &Chunk) -> (Mesh, Mesh) {
        // Fine side: only the seam toward the coarse neighbor is known; the
        // other five faces are exposed by policy so the seam count is isolated.
        let fine_faces: [FaceSlab<CHUNK_EDGE>; 6] = std::array::from_fn(|index| {
            if Face::ALL[index] == face {
                FaceSlab::coarse_occupancy_of(face, coarse_source)
            } else {
                FaceSlab::known_air()
            }
        });
        let fine_mesh =
            mesh_exposed_faces_from_snapshot(&OwnedMeshingSnapshot::new(fine.clone(), fine_faces));
        // Coarse side: toward the fine neighbor the seam uses the coverage mask.
        let coarse_faces: [FaceSlab<COARSE_EDGE>; 6] = std::array::from_fn(|index| {
            if Face::ALL[index] == face.opposite() {
                FaceSlab::fine_coverage_of(face.opposite(), fine)
            } else {
                FaceSlab::known_air()
            }
        });
        let coarse_mesh = mesh_exposed_faces_from_snapshot(&OwnedMeshingSnapshot::new(
            coarse_source.downsample_2x(),
            coarse_faces,
        ));
        (fine_mesh, coarse_mesh)
    }

    fn seam_face_count(mesh: &Mesh, face: Face, plane: f32, axis: usize) -> usize {
        mesh.vertices()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|quad| quad[0].face == face && quad.iter().all(|v| v.position[axis] == plane))
            .count()
    }

    fn seam_axis_and_planes(face: Face) -> (usize, f32, f32) {
        // (axis, fine seam plane, coarse seam plane) in each grid's own cell units.
        match face {
            Face::NegativeX => (0, 0.0, 16.0),
            Face::PositiveX => (0, 32.0, 0.0),
            Face::NegativeY => (1, 0.0, 16.0),
            Face::PositiveY => (1, 32.0, 0.0),
            Face::NegativeZ => (2, 0.0, 16.0),
            Face::PositiveZ => (2, 32.0, 0.0),
        }
    }

    #[test]
    fn derived_coarse_slabs_match_the_full_downsample() -> Result<(), ChunkBoundsError> {
        let chunk = crate::diagnostic_fixture();
        let coarse = chunk.downsample_2x();
        for face in Face::ALL {
            assert_eq!(
                FaceSlab::<COARSE_EDGE>::downsampled_from(face, &chunk),
                FaceSlab::from_neighbor(face, &coarse),
                "coarse slab at {face:?}"
            );
            let occupancy = FaceSlab::<CHUNK_EDGE>::coarse_occupancy_of(face, &chunk);
            let expanded = FaceSlab::from_neighbor(face, &coarse);
            for secondary in 0..CHUNK_EDGE {
                for primary in 0..CHUNK_EDGE {
                    let (fx, fy, fz) = match face {
                        Face::NegativeX | Face::PositiveX => (0, primary, secondary),
                        Face::NegativeY | Face::PositiveY => (primary, 0, secondary),
                        Face::NegativeZ | Face::PositiveZ => (primary, secondary, 0),
                    };
                    let (cx, cy, cz) = (fx / 2, fy / 2, fz / 2);
                    assert_eq!(
                        occupancy.sample(face, LocalCoord::new(fx, fy, fz)?),
                        expanded.sample(face, GridCoord::<COARSE_EDGE>::new(cx, cy, cz)?),
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn mixed_resolution_seams_leave_no_gap_and_no_duplicate_face()
    -> Result<(), Box<dyn std::error::Error>> {
        for face in Face::ALL {
            let fine_local = boundary_coord(face)?;
            let touching = boundary_coord(face.opposite())?;
            // A second fine cell in the same coarse block but not touching the seam cell.
            let (tx, ty, tz) = (touching.x(), touching.y(), touching.z());
            let sibling = match face {
                Face::NegativeX | Face::PositiveX => LocalCoord::new(tx, ty ^ 1, tz)?,
                Face::NegativeY | Face::PositiveY => LocalCoord::new(tx ^ 1, ty, tz)?,
                Face::NegativeZ | Face::PositiveZ => LocalCoord::new(tx ^ 1, ty, tz)?,
            };
            let cases: [(bool, Option<LocalCoord>); 4] = [
                (true, None),            // fine solid, coarse block air
                (false, Some(touching)), // fine air, coarse block solid
                (true, Some(sibling)),   // both solid (block solid via a sibling cell)
                (false, None),           // both air
            ];
            for (fine_solid, coarse_cell) in cases {
                let mut fine = Chunk::empty();
                if fine_solid {
                    assert_eq!(fine.write_local(fine_local, VoxelId(2)), VoxelId::AIR);
                }
                let mut coarse_source = Chunk::empty();
                if let Some(cell) = coarse_cell {
                    assert_eq!(coarse_source.write_local(cell, VoxelId(7)), VoxelId::AIR);
                }
                let (expect_fine, expect_coarse) =
                    mixed_seam_expectation(face, &fine, &coarse_source);
                let (fine_mesh, coarse_mesh) = mixed_seam_meshes(face, &fine, &coarse_source);
                let (axis, fine_plane, coarse_plane) = seam_axis_and_planes(face);
                let fine_count = seam_face_count(&fine_mesh, face, fine_plane, axis);
                let coarse_count =
                    seam_face_count(&coarse_mesh, face.opposite(), coarse_plane, axis);
                assert_eq!(
                    fine_count, expect_fine,
                    "{face:?} fine={fine_solid} coarse={coarse_cell:?}"
                );
                assert_eq!(
                    coarse_count, expect_coarse,
                    "{face:?} fine={fine_solid} coarse={coarse_cell:?}"
                );
                // Never both sides at one location; never visible air without a face.
                let expected_pair = match (fine_solid, coarse_cell.is_some()) {
                    (true, false) => (1, 0),
                    (false, true) => (0, 1),
                    (true, true) => (0, 1),
                    (false, false) => (0, 0),
                };
                assert_eq!((fine_count, coarse_count), expected_pair, "{face:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn coarse_seam_is_suppressed_only_under_full_fine_coverage()
    -> Result<(), Box<dyn std::error::Error>> {
        for face in Face::ALL {
            // The coarse source is solid throughout its touching block so the
            // coarse face candidate exists; the fine side covers 0..=4 cells.
            let touching = boundary_coord(face.opposite())?;
            let (tx, ty, tz) = (touching.x(), touching.y(), touching.z());
            let mut coarse_source = Chunk::empty();
            assert_eq!(
                coarse_source.write_local(touching, VoxelId(7)),
                VoxelId::AIR
            );
            let fine_base = boundary_coord(face)?;
            let (fx, fy, fz) = (fine_base.x(), fine_base.y(), fine_base.z());
            // Fine cells covering coarse block (tx/2, ty/2, tz/2) on the seam layer.
            let cells: Vec<LocalCoord> = (0..4)
                .map(|bit| {
                    let (a, b) = (bit & 1, bit >> 1);
                    match face {
                        Face::NegativeX | Face::PositiveX => {
                            LocalCoord::new(fx, (fy / 2) * 2 + a, (fz / 2) * 2 + b)
                        }
                        Face::NegativeY | Face::PositiveY => {
                            LocalCoord::new((fx / 2) * 2 + a, fy, (fz / 2) * 2 + b)
                        }
                        Face::NegativeZ | Face::PositiveZ => {
                            LocalCoord::new((fx / 2) * 2 + a, (fy / 2) * 2 + b, fz)
                        }
                    }
                })
                .collect::<Result<_, _>>()?;
            // Only the two seam-tangential axes must address the same block; the
            // normal axis differs by construction (touching layer vs seam layer).
            let tangential = |x: usize, y: usize, z: usize| match face {
                Face::NegativeX | Face::PositiveX => (y / 2, z / 2),
                Face::NegativeY | Face::PositiveY => (x / 2, z / 2),
                Face::NegativeZ | Face::PositiveZ => (x / 2, y / 2),
            };
            assert_eq!(tangential(tx, ty, tz), tangential(fx, fy, fz), "same block");
            for covered in 0..=4 {
                let mut fine = Chunk::empty();
                for cell in &cells[..covered] {
                    assert_eq!(fine.write_local(*cell, VoxelId(2)), VoxelId::AIR);
                }
                let (expect_fine, expect_coarse) =
                    mixed_seam_expectation(face, &fine, &coarse_source);
                let (fine_mesh, coarse_mesh) = mixed_seam_meshes(face, &fine, &coarse_source);
                let (axis, fine_plane, coarse_plane) = seam_axis_and_planes(face);
                let fine_count = seam_face_count(&fine_mesh, face, fine_plane, axis);
                let coarse_count =
                    seam_face_count(&coarse_mesh, face.opposite(), coarse_plane, axis);
                assert_eq!(fine_count, expect_fine, "{face:?} coverage {covered}/4");
                assert_eq!(coarse_count, expect_coarse, "{face:?} coverage {covered}/4");
                // Fine cells never emit toward a solid block; the coarse quad is
                // whole while coverage is partial and gone only at 4/4.
                assert_eq!(fine_count, 0, "{face:?} coverage {covered}/4");
                assert_eq!(
                    coarse_count,
                    usize::from(covered < 4),
                    "{face:?} coverage {covered}/4"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn mixed_seam_on_negative_fixture_chunks_matches_the_oracle()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = crate::multichunk_diagnostic_fixture();
        let (negative_coord, negative) = &fixture[0];
        let (origin_coord, origin) = &fixture[1];
        assert_eq!((negative_coord.x, origin_coord.x), (-1, 0));
        // Origin is fine; the negative chunk is presented coarse across NegativeX.
        let face = Face::NegativeX;
        let (expect_fine, expect_coarse) = mixed_seam_expectation(face, origin, negative);
        assert!(
            expect_fine + expect_coarse > 0,
            "fixture seam must be populated"
        );
        let (fine_mesh, coarse_mesh) = mixed_seam_meshes(face, origin, negative);
        let (axis, fine_plane, coarse_plane) = seam_axis_and_planes(face);
        assert_eq!(
            seam_face_count(&fine_mesh, face, fine_plane, axis),
            expect_fine
        );
        assert_eq!(
            seam_face_count(&coarse_mesh, face.opposite(), coarse_plane, axis),
            expect_coarse
        );
        // Same-level Lod0 seam between the same chunks is unchanged from M3A.
        let m3a = mesh_exposed_faces_with_neighbors(
            ChunkNeighborhood::new(origin).with_neighbor(face, negative),
            BoundaryPolicy::Expose,
        )?;
        let owned: [FaceSlab<CHUNK_EDGE>; 6] = std::array::from_fn(|index| {
            if Face::ALL[index] == face {
                FaceSlab::from_neighbor(face, negative)
            } else {
                FaceSlab::known_air()
            }
        });
        assert_eq!(
            mesh_exposed_faces_from_snapshot(&OwnedMeshingSnapshot::new(origin.clone(), owned)),
            m3a
        );
        Ok(())
    }

    fn boundary_coord(face: Face) -> Result<LocalCoord, ChunkBoundsError> {
        boundary_coord_of::<CHUNK_EDGE>(face)
    }

    fn boundary_coord_of<const EDGE: usize>(
        face: Face,
    ) -> Result<GridCoord<EDGE>, ChunkBoundsError> {
        let last = EDGE - 1;
        let (x, y, z) = match face {
            Face::NegativeX => (0, 10, 11),
            Face::PositiveX => (last, 10, 11),
            Face::NegativeY => (10, 0, 11),
            Face::PositiveY => (10, last, 11),
            Face::NegativeZ => (10, 11, 0),
            Face::PositiveZ => (10, 11, last),
        };
        GridCoord::new(x, y, z)
    }

    fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
        [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
    }

    fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
        [
            left[1] * right[2] - left[2] * right[1],
            left[2] * right[0] - left[0] * right[2],
            left[0] * right[1] - left[1] * right[0],
        ]
    }

    fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
        left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
    }
}
