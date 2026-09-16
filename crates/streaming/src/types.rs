use veldwake_voxel::ChunkCoord;

/// Globally unique identity of one tracked-coordinate incarnation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestToken(pub(crate) u64);

impl RequestToken {
    pub const FIRST: Self = Self(1);

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Versioned state of one axial dependency captured by a mesh job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NeighborStamp {
    Resident {
        coord: ChunkCoord,
        token: RequestToken,
        content_generation: u64,
    },
    KnownAbsent {
        coord: ChunkCoord,
        token: RequestToken,
    },
}

/// Complete validity proof attached to a detached mesh job and result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshStamp {
    pub coord: ChunkCoord,
    pub request_token: RequestToken,
    pub mesh_generation: u64,
    pub center_content_generation: u64,
    pub neighbors: [NeighborStamp; 6],
}
