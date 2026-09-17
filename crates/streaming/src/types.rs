use veldwake_voxel::{ChunkCoord, Face};

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

/// The seam rule a mesh job applied toward one neighbor. Only this enters the
/// stamp: two presentations that mesh identically (a content-only neighbor
/// and a rendered neighbor at the center's own level) share a contract, so a
/// neighbor entering render demand at the same level never invalidates a
/// seam that would not change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeamContract {
    /// Neighbor sampled at the center's own resolution.
    Same,
    /// Fine center next to a coarse-presented neighbor: coarse-occupancy rule.
    CoarseNeighbor,
    /// Coarse center next to a fine-presented neighbor: coverage-mask rule.
    FineNeighbor,
}

impl SeamContract {
    #[must_use]
    pub const fn derive(center: LodLevel, neighbor: NeighborPresentation) -> Self {
        match (center, neighbor) {
            (LodLevel::Lod0, NeighborPresentation::Rendered(LodLevel::Lod1)) => {
                Self::CoarseNeighbor
            }
            (LodLevel::Lod1, NeighborPresentation::Rendered(LodLevel::Lod0)) => Self::FineNeighbor,
            _ => Self::Same,
        }
    }
}

/// Versioned state of one axial dependency captured by a mesh job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NeighborStamp {
    Resident {
        coord: ChunkCoord,
        token: RequestToken,
        content_generation: u64,
        seam: SeamContract,
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

impl MeshStamp {
    /// The dependency stamp captured toward `face` (`Face::ALL` order).
    #[must_use]
    pub const fn neighbor(&self, face: Face) -> &NeighborStamp {
        &self.neighbors[face as usize]
    }

    /// Everything that determines the geometry: the stamp without its mesh
    /// generation. Two stamps with equal keys describe identical meshes.
    #[must_use]
    pub fn geometry_key(&self) -> MeshStamp {
        MeshStamp {
            mesh_generation: 0,
            ..*self
        }
    }

    /// True when `other` differs from `self` only by presentation: the
    /// center level or a neighbor's seam contract. Tokens, content
    /// generations, and neighbor identities are identical, so the old mesh is
    /// still a correct rendering of the same data.
    #[must_use]
    pub fn differs_only_by_presentation(&self, other: &MeshStamp) -> bool {
        if self.coord != other.coord
            || self.request_token != other.request_token
            || self.center_content_generation != other.center_content_generation
        {
            return false;
        }
        self.neighbors
            .iter()
            .zip(other.neighbors.iter())
            .all(|(mine, theirs)| match (mine, theirs) {
                (
                    NeighborStamp::Resident {
                        coord: a,
                        token: ta,
                        content_generation: ga,
                        ..
                    },
                    NeighborStamp::Resident {
                        coord: b,
                        token: tb,
                        content_generation: gb,
                        ..
                    },
                ) => a == b && ta == tb && ga == gb,
                (
                    NeighborStamp::KnownAbsent {
                        coord: a,
                        token: ta,
                    },
                    NeighborStamp::KnownAbsent {
                        coord: b,
                        token: tb,
                    },
                ) => a == b && ta == tb,
                _ => false,
            })
    }
}
