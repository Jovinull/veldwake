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

/// Level of detail a render-demand chunk is meshed at.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LodLevel {
    /// Full `32³` resolution.
    Lod0,
    /// `16³` grid derived by `Chunk::downsample_2x`.
    Lod1,
}

/// What a neighbor contributes to a seam: content only, or a presented level
/// that decides the mixed-resolution seam rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NeighborPresentation {
    /// Dependency/retention chunk: it supplies voxels at the center's own
    /// resolution and never carries a render level.
    ContentOnly,
    Rendered(LodLevel),
}

/// Versioned state of one axial dependency captured by a mesh job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NeighborStamp {
    Resident {
        coord: ChunkCoord,
        token: RequestToken,
        content_generation: u64,
        presentation: NeighborPresentation,
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
    pub lod: LodLevel,
    pub mesh_generation: u64,
    pub center_content_generation: u64,
    pub neighbors: [NeighborStamp; 6],
}
