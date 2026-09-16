use std::error::Error;
use std::time::Instant;

use veldwake_voxel::{
    CHUNK_BYTES, CHUNK_EDGE, Chunk, VoxelId, diagnostic_fixture, fingerprint, mesh_exposed_faces,
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

    Ok(())
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
