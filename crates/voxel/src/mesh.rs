use std::mem::size_of_val;

use crate::{CHUNK_EDGE, Chunk, LocalCoord, VoxelId};

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
/// For this isolated M2 chunk only, a neighbor outside the chunk is considered
/// air. Vertices for each quad are counter-clockwise when viewed from outside.
#[must_use]
pub fn mesh_exposed_faces(chunk: &Chunk) -> Mesh {
    let mut mesh = Mesh::default();

    for z in 0..CHUNK_EDGE {
        for y in 0..CHUNK_EDGE {
            for x in 0..CHUNK_EDGE {
                let coord = local_coord_from_loop(x, y, z);
                let voxel = chunk.read_local(coord);
                if voxel.is_air() {
                    continue;
                }

                for face in FACES {
                    if neighbor_is_air(chunk, x, y, z, face) {
                        emit_quad(&mut mesh, x, y, z, face, voxel);
                    }
                }
            }
        }
    }

    mesh
}

const FACES: [Face; 6] = [
    Face::NegativeX,
    Face::PositiveX,
    Face::NegativeY,
    Face::PositiveY,
    Face::NegativeZ,
    Face::PositiveZ,
];

fn local_coord_from_loop(x: usize, y: usize, z: usize) -> LocalCoord {
    match LocalCoord::new(x, y, z) {
        Ok(coord) => coord,
        Err(error) => unreachable!("mesher loop generated invalid coordinate: {error}"),
    }
}

fn neighbor_is_air(chunk: &Chunk, x: usize, y: usize, z: usize, face: Face) -> bool {
    let neighbor = match face {
        Face::NegativeX => x.checked_sub(1).map(|next| (next, y, z)),
        Face::PositiveX => (x + 1 < CHUNK_EDGE).then_some((x + 1, y, z)),
        Face::NegativeY => y.checked_sub(1).map(|next| (x, next, z)),
        Face::PositiveY => (y + 1 < CHUNK_EDGE).then_some((x, y + 1, z)),
        Face::NegativeZ => z.checked_sub(1).map(|next| (x, y, next)),
        Face::PositiveZ => (z + 1 < CHUNK_EDGE).then_some((x, y, z + 1)),
    };

    neighbor.is_none_or(|(next_x, next_y, next_z)| {
        chunk
            .read_local(local_coord_from_loop(next_x, next_y, next_z))
            .is_air()
    })
}

fn emit_quad(mesh: &mut Mesh, x: usize, y: usize, z: usize, face: Face, voxel: VoxelId) {
    let base = match u32::try_from(mesh.vertices.len()) {
        Ok(base) => base,
        Err(_) => unreachable!("M2 chunk mesh exceeds u32 vertex addressing"),
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
    use crate::ChunkBoundsError;

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
        for face in FACES {
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
