use std::error::Error;
use std::time::Instant;

use veldwake_voxel::{
    BoundaryPolicy, CHUNK_BYTES, CHUNK_EDGE, Chunk, ChunkCoord, ChunkNeighborhood, Face, VoxelId,
    diagnostic_fixture, fingerprint, mesh_exposed_faces, mesh_exposed_faces_with_neighbors,
    multichunk_diagnostic_fixture, multichunk_fingerprint,
};

fn main() -> Result<(), Box<dyn Error>> {
    let fixtures = [
        ("empty", Chunk::empty()),
        ("single", single_voxel()?),
        ("solid", solid_chunk()?),
        ("diagnostic", diagnostic_fixture()),
    ];

    for (name, chunk) in fixtures {
        let start = Instant::now();
        let mesh = mesh_exposed_faces(&chunk);
        let elapsed = start.elapsed();
        println!(
            "fixture={name} solids={} quads={} vertices={} indices={} chunk_bytes={} mesh_bytes={} mesh_time_us={} fingerprint=0x{:016x}",
            chunk.solid_count(),
            mesh.quad_count(),
            mesh.vertices().len(),
            mesh.indices().len(),
            CHUNK_BYTES,
            mesh.payload_bytes(),
            elapsed.as_micros(),
            fingerprint(&chunk),
        );
    }

    let chunks = multichunk_diagnostic_fixture();
    let start = Instant::now();
    let mut solids = 0;
    let mut quads = 0;
    let mut vertices = 0;
    let mut indices = 0;
    let mut mesh_bytes = 0;
    for (coord, chunk) in &chunks {
        let mesh = mesh_exposed_faces_with_neighbors(
            neighborhood_for(*coord, chunk, &chunks),
            BoundaryPolicy::Expose,
        )?;
        solids += chunk.solid_count();
        quads += mesh.quad_count();
        vertices += mesh.vertices().len();
        indices += mesh.indices().len();
        mesh_bytes += mesh.payload_bytes();
    }
    let elapsed = start.elapsed();
    println!(
        "fixture=multichunk chunks={} solids={solids} quads={quads} vertices={vertices} indices={indices} chunk_bytes={} mesh_bytes={mesh_bytes} mesh_time_us={} fingerprint=0x{:016x}",
        chunks.len(),
        chunks.len() * CHUNK_BYTES,
        elapsed.as_micros(),
        multichunk_fingerprint(&chunks),
    );

    Ok(())
}

fn neighborhood_for<'a>(
    coord: ChunkCoord,
    center: &'a Chunk,
    chunks: &'a [(ChunkCoord, Chunk)],
) -> ChunkNeighborhood<'a> {
    let mut neighborhood = ChunkNeighborhood::new(center);
    for face in Face::ALL {
        let Some(neighbor_coord) = coord.neighbor(face) else {
            continue;
        };
        if let Some((_, neighbor)) = chunks
            .iter()
            .find(|(candidate, _)| *candidate == neighbor_coord)
        {
            neighborhood = neighborhood.with_neighbor(face, neighbor);
        }
    }
    neighborhood
}

fn single_voxel() -> Result<Chunk, Box<dyn Error>> {
    let mut chunk = Chunk::empty();
    chunk.write(16, 16, 16, VoxelId(1))?;
    Ok(chunk)
}

fn solid_chunk() -> Result<Chunk, Box<dyn Error>> {
    let mut chunk = Chunk::empty();
    for z in 0..CHUNK_EDGE {
        for y in 0..CHUNK_EDGE {
            for x in 0..CHUNK_EDGE {
                chunk.write(x, y, z, VoxelId(1))?;
            }
        }
    }
    Ok(chunk)
}
