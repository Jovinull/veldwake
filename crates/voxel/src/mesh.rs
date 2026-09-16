use std::{fmt, mem::size_of_val};

use crate::{CHUNK_EDGE, DenseGrid, GridCoord, VoxelId};

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

    fn sample(&self, face: Face, coord: GridCoord<EDGE>) -> VoxelId {
        self.cells
            .as_ref()
            .map_or(VoxelId::AIR, |cells| cells[slab_index(face, coord)])
    }
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
