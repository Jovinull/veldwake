//! The experimental on-disk chunk-cache entry format.
//!
//! This is a **cache** format, not a save format. Every entry is derivable
//! again from the source that produced it, so a rejected entry is always
//! discardable and never a data loss. Nothing here promises save-game or
//! network compatibility, and no migration path is implied: an unknown or
//! mismatched header is a cache miss, never an upgrade.
//!
//! The layout is explicit and endian-fixed. No Rust struct is written
//! directly, no `transmute`, no `repr` dependency, and no native-endian
//! integer reaches disk. Every field is read and written byte by byte in
//! little-endian order.
//!
//! ```text
//! offset size field
//!      0    8 magic                b"VWKCACHE"
//!      8    2 format_version       u16
//!     10    2 chunk_edge           u16   cells per chunk edge
//!     12    2 voxel_encoding       u16   cell encoding discriminant
//!     14    1 entry_kind           u8    0 KnownAbsent, 1 Present
//!     15    1 payload_encoding     u8    0 Raw, 1 Rle
//!     16   12 chunk coord          3 x i32
//!     28    8 source_fingerprint   u64
//!     36    4 payload_len          u32
//!     40    8 checksum             u64   FNV-1a over bytes 0..40 plus payload
//!     48    n payload
//! ```
//!
//! A file is exactly `HEADER_BYTES + payload_len` long. Anything shorter is
//! truncated and anything longer carries trailing bytes; both are rejected.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use veldwake_voxel::{CHUNK_EDGE, Chunk, ChunkCoord, GridCoord, LocalCoord, VoxelId};

use crate::{hash::fnv1a64, source::SourceChunk};

/// Identifies this format to anything that finds the file.
pub(crate) const MAGIC: [u8; 8] = *b"VWKCACHE";
/// Bumped whenever the byte layout or its meaning changes. An entry written
/// by another version is a miss, never a migration.
pub(crate) const FORMAT_VERSION: u16 = 1;
/// One `u16` little-endian cell per voxel, in `x + EDGE * (y + EDGE * z)`
/// order. The discriminant exists so a future cell encoding is a rejection
/// rather than a silent misread.
pub(crate) const VOXEL_ENCODING_U16LE: u16 = 1;
/// Fixed header size in bytes.
pub(crate) const HEADER_BYTES: usize = 48;

const OFFSET_VERSION: usize = 8;
const OFFSET_EDGE: usize = 10;
const OFFSET_VOXEL_ENCODING: usize = 12;
const OFFSET_ENTRY_KIND: usize = 14;
const OFFSET_PAYLOAD_ENCODING: usize = 15;
const OFFSET_COORD: usize = 16;
const OFFSET_FINGERPRINT: usize = 28;
const OFFSET_PAYLOAD_LEN: usize = 36;
const OFFSET_CHECKSUM: usize = 40;

/// Bytes per run in the run-length payload encoding: `u32` count, `u16` value.
const RLE_RUN_BYTES: usize = 6;
/// Largest valid entry under either current payload encoding. This bound is
/// also enforced while reading so a hostile cache file cannot make a worker
/// allocate an arbitrary amount before validation.
pub(crate) const MAX_ENTRY_BYTES: usize = HEADER_BYTES + Chunk::VOLUME * RLE_RUN_BYTES;

/// How the cell payload is stored.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PayloadEncoding {
    /// Every cell as a `u16`, fixed size.
    #[default]
    Raw,
    /// Run-length pairs of `(count, value)`.
    Rle,
}

impl PayloadEncoding {
    const fn code(self) -> u8 {
        match self {
            Self::Raw => 0,
            Self::Rle => 1,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Raw),
            1 => Some(Self::Rle),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Rle => "rle",
        }
    }
}

/// Whether an entry carries chunk content or records authoritative absence.
///
/// Absence is stored explicitly. A missing file is a cache miss and must never
/// be read as `KnownAbsent`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    KnownAbsent,
    Present,
}

impl EntryKind {
    const fn code(self) -> u8 {
        match self {
            Self::KnownAbsent => 0,
            Self::Present => 1,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::KnownAbsent),
            1 => Some(Self::Present),
            _ => None,
        }
    }
}

/// What a cache entry must agree with to be usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EntryIdentity {
    pub(crate) coord: ChunkCoord,
    /// Identity of the source that produced the content. A different
    /// fingerprint invalidates the entry; it never silently passes.
    pub(crate) source_fingerprint: u64,
}

/// Why an entry could not be used. Every variant is a cache miss with a
/// reason, never a panic and never an interpretation of the content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CacheFormatError {
    Truncated {
        len: usize,
        needed: usize,
    },
    BadMagic,
    UnknownFormatVersion {
        found: u16,
    },
    ChunkEdgeMismatch {
        expected: u16,
        found: u16,
    },
    VoxelEncodingMismatch {
        expected: u16,
        found: u16,
    },
    UnknownEntryKind {
        code: u8,
    },
    UnknownPayloadEncoding {
        code: u8,
    },
    PayloadLengthMismatch {
        declared: usize,
        actual: usize,
    },
    PayloadTooLarge {
        len: usize,
    },
    TrailingBytes {
        extra: usize,
    },
    ChecksumMismatch {
        declared: u64,
        computed: u64,
    },
    CoordMismatch {
        expected: ChunkCoord,
        found: ChunkCoord,
    },
    FingerprintMismatch {
        expected: u64,
        found: u64,
    },
    AbsentWithPayload {
        len: usize,
    },
    PayloadCellCount {
        expected: usize,
        found: usize,
    },
    EmptyRun,
}

impl CacheFormatError {
    /// Whether the entry is merely from another version or source identity, or
    /// genuinely damaged. Both are discarded; the counters stay separate so a
    /// real corruption problem is never hidden inside routine invalidation.
    #[must_use]
    pub const fn is_stale(self) -> bool {
        matches!(
            self,
            Self::UnknownFormatVersion { .. }
                | Self::ChunkEdgeMismatch { .. }
                | Self::VoxelEncodingMismatch { .. }
                | Self::UnknownPayloadEncoding { .. }
                | Self::FingerprintMismatch { .. }
        )
    }
}

impl Display for CacheFormatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { len, needed } => {
                write!(formatter, "entry is {len} bytes, needs at least {needed}")
            }
            Self::BadMagic => formatter.write_str("entry does not start with the cache magic"),
            Self::UnknownFormatVersion { found } => {
                write!(formatter, "unknown cache format version {found}")
            }
            Self::ChunkEdgeMismatch { expected, found } => {
                write!(
                    formatter,
                    "entry chunk edge {found}, runtime uses {expected}"
                )
            }
            Self::VoxelEncodingMismatch { expected, found } => write!(
                formatter,
                "entry voxel encoding {found}, runtime uses {expected}"
            ),
            Self::UnknownEntryKind { code } => write!(formatter, "unknown entry kind {code}"),
            Self::UnknownPayloadEncoding { code } => {
                write!(formatter, "unknown payload encoding {code}")
            }
            Self::PayloadLengthMismatch { declared, actual } => write!(
                formatter,
                "entry declares {declared} payload bytes but holds {actual}"
            ),
            Self::PayloadTooLarge { len } => {
                write!(formatter, "payload length {len} does not fit the format")
            }
            Self::TrailingBytes { extra } => {
                write!(formatter, "entry has {extra} trailing bytes")
            }
            Self::ChecksumMismatch { declared, computed } => write!(
                formatter,
                "checksum {declared:#018x} does not match {computed:#018x}"
            ),
            Self::CoordMismatch { expected, found } => write!(
                formatter,
                "entry holds chunk ({}, {}, {}), expected ({}, {}, {})",
                found.x, found.y, found.z, expected.x, expected.y, expected.z
            ),
            Self::FingerprintMismatch { expected, found } => write!(
                formatter,
                "entry source fingerprint {found:#018x}, expected {expected:#018x}"
            ),
            Self::AbsentWithPayload { len } => {
                write!(formatter, "known-absent entry carries {len} payload bytes")
            }
            Self::PayloadCellCount { expected, found } => write!(
                formatter,
                "payload decodes to {found} cells, expected {expected}"
            ),
            Self::EmptyRun => formatter.write_str("payload contains a zero-length run"),
        }
    }
}

impl Error for CacheFormatError {}

/// Integrity value over the header and payload.
fn checksum(bytes: &[u8]) -> u64 {
    fnv1a64(bytes)
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_i32(bytes: &mut [u8], offset: usize, value: i32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut value = [0_u8; 8];
    value.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(value)
}

/// The chunk edge as it appears in the header. `CHUNK_EDGE` is a `usize`
/// constant well below `u16::MAX`, so this conversion cannot truncate.
const fn header_chunk_edge() -> u16 {
    CHUNK_EDGE as u16
}

/// Encodes one source result as a complete cache entry.
pub(crate) fn encode(
    identity: EntryIdentity,
    source: &SourceChunk,
    encoding: PayloadEncoding,
) -> Result<Vec<u8>, CacheFormatError> {
    let (kind, payload, payload_encoding) = match source {
        // Absence has no cells, so it is always stored raw and empty.
        SourceChunk::KnownAbsent => (EntryKind::KnownAbsent, Vec::new(), PayloadEncoding::Raw),
        SourceChunk::Present(chunk) => (
            EntryKind::Present,
            encode_payload(chunk, encoding)?,
            encoding,
        ),
    };

    let mut bytes = vec![0_u8; HEADER_BYTES + payload.len()];
    bytes[..MAGIC.len()].copy_from_slice(&MAGIC);
    put_u16(&mut bytes, OFFSET_VERSION, FORMAT_VERSION);
    put_u16(&mut bytes, OFFSET_EDGE, header_chunk_edge());
    put_u16(&mut bytes, OFFSET_VOXEL_ENCODING, VOXEL_ENCODING_U16LE);
    bytes[OFFSET_ENTRY_KIND] = kind.code();
    bytes[OFFSET_PAYLOAD_ENCODING] = payload_encoding.code();
    put_i32(&mut bytes, OFFSET_COORD, identity.coord.x);
    put_i32(&mut bytes, OFFSET_COORD + 4, identity.coord.y);
    put_i32(&mut bytes, OFFSET_COORD + 8, identity.coord.z);
    put_u64(&mut bytes, OFFSET_FINGERPRINT, identity.source_fingerprint);
    // This is unreachable for today's bounded encodings, but the persistent
    // format must fail explicitly if a future encoder violates that bound.
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| CacheFormatError::PayloadTooLarge { len: payload.len() })?;
    put_u32(&mut bytes, OFFSET_PAYLOAD_LEN, payload_len);
    bytes[HEADER_BYTES..].copy_from_slice(&payload);

    let mut digest_input = Vec::with_capacity(OFFSET_CHECKSUM + payload.len());
    digest_input.extend_from_slice(&bytes[..OFFSET_CHECKSUM]);
    digest_input.extend_from_slice(&payload);
    put_u64(&mut bytes, OFFSET_CHECKSUM, checksum(&digest_input));
    Ok(bytes)
}

/// Decodes a cache entry, proving it belongs to `expected` before trusting a
/// single cell. Any failure is a typed error: malformed external input never
/// panics and never becomes `KnownAbsent` by accident.
pub(crate) fn decode(
    bytes: &[u8],
    expected: EntryIdentity,
) -> Result<SourceChunk, CacheFormatError> {
    if bytes.len() < HEADER_BYTES {
        return Err(CacheFormatError::Truncated {
            len: bytes.len(),
            needed: HEADER_BYTES,
        });
    }
    if bytes[..MAGIC.len()] != MAGIC {
        return Err(CacheFormatError::BadMagic);
    }
    let declared = read_u32(bytes, OFFSET_PAYLOAD_LEN) as usize;
    let available = bytes.len() - HEADER_BYTES;
    if declared > available {
        return Err(CacheFormatError::PayloadLengthMismatch {
            declared,
            actual: available,
        });
    }
    if declared < available {
        return Err(CacheFormatError::TrailingBytes {
            extra: available - declared,
        });
    }
    let payload = &bytes[HEADER_BYTES..];

    let declared_checksum = read_u64(bytes, OFFSET_CHECKSUM);
    let mut digest_input = Vec::with_capacity(OFFSET_CHECKSUM + payload.len());
    digest_input.extend_from_slice(&bytes[..OFFSET_CHECKSUM]);
    digest_input.extend_from_slice(payload);
    let computed = checksum(&digest_input);
    if declared_checksum != computed {
        return Err(CacheFormatError::ChecksumMismatch {
            declared: declared_checksum,
            computed,
        });
    }

    // Structural fields are interpreted only after integrity succeeds. This
    // keeps an intact entry from an older identity classified as stale while
    // a flipped version/encoding bit is classified as corruption.
    let version = read_u16(bytes, OFFSET_VERSION);
    if version != FORMAT_VERSION {
        return Err(CacheFormatError::UnknownFormatVersion { found: version });
    }
    let edge = read_u16(bytes, OFFSET_EDGE);
    if edge != header_chunk_edge() {
        return Err(CacheFormatError::ChunkEdgeMismatch {
            expected: header_chunk_edge(),
            found: edge,
        });
    }
    let voxel_encoding = read_u16(bytes, OFFSET_VOXEL_ENCODING);
    if voxel_encoding != VOXEL_ENCODING_U16LE {
        return Err(CacheFormatError::VoxelEncodingMismatch {
            expected: VOXEL_ENCODING_U16LE,
            found: voxel_encoding,
        });
    }
    let kind = EntryKind::from_code(bytes[OFFSET_ENTRY_KIND]).ok_or(
        CacheFormatError::UnknownEntryKind {
            code: bytes[OFFSET_ENTRY_KIND],
        },
    )?;
    let payload_encoding = PayloadEncoding::from_code(bytes[OFFSET_PAYLOAD_ENCODING]).ok_or(
        CacheFormatError::UnknownPayloadEncoding {
            code: bytes[OFFSET_PAYLOAD_ENCODING],
        },
    )?;

    let found = ChunkCoord::new(
        read_i32(bytes, OFFSET_COORD),
        read_i32(bytes, OFFSET_COORD + 4),
        read_i32(bytes, OFFSET_COORD + 8),
    );
    if found != expected.coord {
        return Err(CacheFormatError::CoordMismatch {
            expected: expected.coord,
            found,
        });
    }
    let fingerprint = read_u64(bytes, OFFSET_FINGERPRINT);
    if fingerprint != expected.source_fingerprint {
        return Err(CacheFormatError::FingerprintMismatch {
            expected: expected.source_fingerprint,
            found: fingerprint,
        });
    }

    match kind {
        EntryKind::KnownAbsent => {
            if payload.is_empty() {
                Ok(SourceChunk::KnownAbsent)
            } else {
                Err(CacheFormatError::AbsentWithPayload { len: payload.len() })
            }
        }
        EntryKind::Present => Ok(SourceChunk::Present(decode_payload(
            payload,
            payload_encoding,
        )?)),
    }
}

fn encode_payload(chunk: &Chunk, encoding: PayloadEncoding) -> Result<Vec<u8>, CacheFormatError> {
    match encoding {
        PayloadEncoding::Raw => {
            let mut payload = Vec::with_capacity(Chunk::BYTES);
            for index in 0..Chunk::VOLUME {
                payload.extend_from_slice(&cell_at(chunk, index)?.0.to_le_bytes());
            }
            Ok(payload)
        }
        PayloadEncoding::Rle => {
            let mut payload = Vec::new();
            let mut run_value = cell_at(chunk, 0)?;
            let mut run_len: u32 = 0;
            for index in 0..Chunk::VOLUME {
                let value = cell_at(chunk, index)?;
                if value == run_value && run_len < u32::MAX {
                    run_len += 1;
                    continue;
                }
                push_run(&mut payload, run_len, run_value);
                run_value = value;
                run_len = 1;
            }
            push_run(&mut payload, run_len, run_value);
            Ok(payload)
        }
    }
}

fn push_run(payload: &mut Vec<u8>, len: u32, value: VoxelId) {
    payload.extend_from_slice(&len.to_le_bytes());
    payload.extend_from_slice(&value.0.to_le_bytes());
}

fn decode_payload(payload: &[u8], encoding: PayloadEncoding) -> Result<Chunk, CacheFormatError> {
    let mut chunk = Chunk::empty();
    match encoding {
        PayloadEncoding::Raw => {
            if payload.len() != Chunk::BYTES {
                return Err(CacheFormatError::PayloadCellCount {
                    expected: Chunk::VOLUME,
                    found: payload.len() / size_of::<u16>(),
                });
            }
            let (cells, _) = payload.as_chunks::<{ size_of::<u16>() }>();
            for (index, cell) in cells.iter().enumerate() {
                let value = VoxelId(u16::from_le_bytes(*cell));
                write_cell(&mut chunk, index, value)?;
            }
        }
        PayloadEncoding::Rle => {
            if !payload.len().is_multiple_of(RLE_RUN_BYTES) {
                return Err(CacheFormatError::PayloadCellCount {
                    expected: Chunk::VOLUME,
                    found: payload.len() / RLE_RUN_BYTES,
                });
            }
            let mut index = 0_usize;
            let (runs, _) = payload.as_chunks::<RLE_RUN_BYTES>();
            for run in runs {
                let len = u32::from_le_bytes([run[0], run[1], run[2], run[3]]) as usize;
                if len == 0 {
                    return Err(CacheFormatError::EmptyRun);
                }
                let value = VoxelId(u16::from_le_bytes([run[4], run[5]]));
                let end = index.saturating_add(len);
                if end > Chunk::VOLUME {
                    return Err(CacheFormatError::PayloadCellCount {
                        expected: Chunk::VOLUME,
                        found: end,
                    });
                }
                for cell in index..end {
                    write_cell(&mut chunk, cell, value)?;
                }
                index = end;
            }
            if index != Chunk::VOLUME {
                return Err(CacheFormatError::PayloadCellCount {
                    expected: Chunk::VOLUME,
                    found: index,
                });
            }
        }
    }
    Ok(chunk)
}

/// Linear cell order is the grid's own: `x + EDGE * (y + EDGE * z)`.
///
/// Run lengths in a decoded payload come from the file, so an out-of-range
/// index is external input rather than an internal invariant and is returned
/// as an error instead of panicking.
fn cell_coord(index: usize) -> Result<LocalCoord, CacheFormatError> {
    let x = index % CHUNK_EDGE;
    let y = (index / CHUNK_EDGE) % CHUNK_EDGE;
    let z = index / (CHUNK_EDGE * CHUNK_EDGE);
    GridCoord::new(x, y, z).map_err(|_| CacheFormatError::PayloadCellCount {
        expected: Chunk::VOLUME,
        found: index,
    })
}

fn cell_at(chunk: &Chunk, index: usize) -> Result<VoxelId, CacheFormatError> {
    Ok(chunk.read_local(cell_coord(index)?))
}

fn write_cell(chunk: &mut Chunk, index: usize, value: VoxelId) -> Result<(), CacheFormatError> {
    if value.is_air() {
        return Ok(());
    }
    let previous = chunk.write_local(cell_coord(index)?, value);
    debug_assert!(previous.is_air(), "each cell is written once per decode");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use veldwake_voxel::fingerprint;

    fn identity(coord: ChunkCoord) -> EntryIdentity {
        EntryIdentity {
            coord,
            source_fingerprint: 0x0123_4567_89ab_cdef,
        }
    }

    fn sample_chunk() -> Chunk {
        let mut chunk = Chunk::empty();
        for (index, id) in [(0_usize, 1_u16), (37, 2), (4095, 7), (32767, 9)] {
            if let Err(error) = write_cell(&mut chunk, index, VoxelId(id)) {
                panic!("fixture cell {index} rejected: {error}");
            }
        }
        chunk
    }

    fn sealed(identity: EntryIdentity, source: &SourceChunk, encoding: PayloadEncoding) -> Vec<u8> {
        match encode(identity, source, encoding) {
            Ok(bytes) => bytes,
            Err(error) => panic!("encoding a valid chunk failed: {error}"),
        }
    }

    /// Decodes and requires a rejection. The workspace forbids `expect`, and a
    /// named helper also keeps every rejection assertion phrased the same way.
    fn rejected(bytes: &[u8], expected: EntryIdentity, what: &str) -> CacheFormatError {
        match decode(bytes, expected) {
            Err(error) => error,
            Ok(value) => panic!("{what} was accepted: {value:?}"),
        }
    }

    fn cell(chunk: &Chunk, index: usize) -> VoxelId {
        match cell_at(chunk, index) {
            Ok(value) => value,
            Err(error) => panic!("reading cell {index} failed: {error}"),
        }
    }

    #[test]
    fn the_header_layout_is_fixed_and_little_endian() {
        let coord = ChunkCoord::new(-3, 1, 2);
        let bytes = sealed(
            identity(coord),
            &SourceChunk::KnownAbsent,
            PayloadEncoding::Raw,
        );
        assert_eq!(bytes.len(), HEADER_BYTES);
        assert_eq!(bytes[..8], MAGIC);
        assert_eq!(read_u16(&bytes, OFFSET_VERSION), FORMAT_VERSION);
        assert_eq!(read_u16(&bytes, OFFSET_EDGE), 32);
        assert_eq!(
            read_u16(&bytes, OFFSET_VOXEL_ENCODING),
            VOXEL_ENCODING_U16LE
        );
        assert_eq!(bytes[OFFSET_ENTRY_KIND], 0);
        assert_eq!(bytes[OFFSET_PAYLOAD_ENCODING], 0);
        assert_eq!(read_i32(&bytes, OFFSET_COORD), -3);
        assert_eq!(read_i32(&bytes, OFFSET_COORD + 4), 1);
        assert_eq!(read_i32(&bytes, OFFSET_COORD + 8), 2);
        assert_eq!(read_u32(&bytes, OFFSET_PAYLOAD_LEN), 0);
        // The negative coordinate is two's complement little-endian, not a
        // platform-dependent byte order.
        assert_eq!(
            &bytes[OFFSET_COORD..OFFSET_COORD + 4],
            &[0xfd, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn present_chunks_round_trip_byte_exactly_in_both_encodings() {
        let chunk = sample_chunk();
        let before = fingerprint(&chunk);
        for encoding in [PayloadEncoding::Raw, PayloadEncoding::Rle] {
            for coord in [
                ChunkCoord::new(0, 0, 0),
                ChunkCoord::new(-1, -1, -1),
                ChunkCoord::new(4, -1, -4),
                ChunkCoord::new(i32::MIN, i32::MAX, i32::MIN),
            ] {
                let bytes = sealed(
                    identity(coord),
                    &SourceChunk::Present(chunk.clone()),
                    encoding,
                );
                let decoded = match decode(&bytes, identity(coord)) {
                    Ok(SourceChunk::Present(decoded)) => decoded,
                    other => panic!("{encoding:?} at {coord:?} did not round trip: {other:?}"),
                };
                assert_eq!(decoded.solid_count(), chunk.solid_count());
                assert_eq!(fingerprint(&decoded), before, "{encoding:?} at {coord:?}");
                for index in 0..Chunk::VOLUME {
                    assert_eq!(cell(&decoded, index), cell(&chunk, index));
                }
            }
        }
    }

    #[test]
    fn an_empty_chunk_and_known_absence_are_different_entries() {
        let coord = ChunkCoord::new(2, 0, -2);
        let empty = sealed(
            identity(coord),
            &SourceChunk::Present(Chunk::empty()),
            PayloadEncoding::Raw,
        );
        let absent = sealed(
            identity(coord),
            &SourceChunk::KnownAbsent,
            PayloadEncoding::Raw,
        );
        assert_ne!(empty, absent);
        assert!(matches!(
            decode(&empty, identity(coord)),
            Ok(SourceChunk::Present(_))
        ));
        assert_eq!(
            decode(&absent, identity(coord)),
            Ok(SourceChunk::KnownAbsent)
        );
    }

    #[test]
    fn every_short_header_and_truncated_payload_is_rejected() {
        let coord = ChunkCoord::default();
        let bytes = sealed(
            identity(coord),
            &SourceChunk::Present(sample_chunk()),
            PayloadEncoding::Rle,
        );
        for len in 0..HEADER_BYTES {
            let error = rejected(&bytes[..len], identity(coord), "a truncated entry");
            assert!(matches!(error, CacheFormatError::Truncated { .. }));
        }
        for len in [HEADER_BYTES, bytes.len() - 1] {
            let error = rejected(&bytes[..len], identity(coord), "a truncated payload");
            assert!(matches!(
                error,
                CacheFormatError::PayloadLengthMismatch { .. }
            ));
        }
    }

    #[test]
    fn structural_mismatches_are_typed_and_classified() {
        let coord = ChunkCoord::new(1, 0, 1);
        let good = sealed(
            identity(coord),
            &SourceChunk::Present(sample_chunk()),
            PayloadEncoding::Raw,
        );

        let mut bad_magic = good.clone();
        bad_magic[0] = b'X';
        assert_eq!(
            decode(&bad_magic, identity(coord)),
            Err(CacheFormatError::BadMagic)
        );
        assert!(!CacheFormatError::BadMagic.is_stale());

        let mut bad_version = good.clone();
        put_u16(&mut bad_version, OFFSET_VERSION, FORMAT_VERSION + 1);
        reseal(&mut bad_version);
        let error = rejected(&bad_version, identity(coord), "an unknown version");
        assert_eq!(error, CacheFormatError::UnknownFormatVersion { found: 2 });
        assert!(error.is_stale());

        let mut bad_edge = good.clone();
        put_u16(&mut bad_edge, OFFSET_EDGE, 16);
        reseal(&mut bad_edge);
        let error = rejected(&bad_edge, identity(coord), "a foreign chunk edge");
        assert!(matches!(
            error,
            CacheFormatError::ChunkEdgeMismatch { found: 16, .. }
        ));
        assert!(error.is_stale());

        let mut bad_cells = good.clone();
        put_u16(&mut bad_cells, OFFSET_VOXEL_ENCODING, 99);
        reseal(&mut bad_cells);
        let error = rejected(&bad_cells, identity(coord), "a foreign cell encoding");
        assert!(matches!(
            error,
            CacheFormatError::VoxelEncodingMismatch { found: 99, .. }
        ));
        assert!(error.is_stale());

        let mut bad_kind = good.clone();
        bad_kind[OFFSET_ENTRY_KIND] = 7;
        reseal(&mut bad_kind);
        assert_eq!(
            decode(&bad_kind, identity(coord)),
            Err(CacheFormatError::UnknownEntryKind { code: 7 })
        );

        let mut bad_payload_encoding = good.clone();
        bad_payload_encoding[OFFSET_PAYLOAD_ENCODING] = 5;
        reseal(&mut bad_payload_encoding);
        let error = rejected(
            &bad_payload_encoding,
            identity(coord),
            "an unknown payload encoding",
        );
        assert_eq!(error, CacheFormatError::UnknownPayloadEncoding { code: 5 });
        assert!(error.is_stale());
    }

    #[test]
    fn damaged_structural_field_is_corrupt_not_stale() {
        let coord = ChunkCoord::new(1, 0, 1);
        let mut damaged = sealed(
            identity(coord),
            &SourceChunk::Present(sample_chunk()),
            PayloadEncoding::Raw,
        );
        put_u16(&mut damaged, OFFSET_VERSION, FORMAT_VERSION + 1);
        let error = rejected(&damaged, identity(coord), "a damaged version field");
        assert!(matches!(error, CacheFormatError::ChecksumMismatch { .. }));
        assert!(!error.is_stale());
    }

    #[test]
    fn length_checksum_and_identity_are_each_rejected_separately() {
        let coord = ChunkCoord::new(-2, 1, 3);
        let good = sealed(
            identity(coord),
            &SourceChunk::Present(sample_chunk()),
            PayloadEncoding::Rle,
        );

        let mut impossible_len = good.clone();
        put_u32(&mut impossible_len, OFFSET_PAYLOAD_LEN, u32::MAX);
        assert!(matches!(
            decode(&impossible_len, identity(coord)),
            Err(CacheFormatError::PayloadLengthMismatch { .. })
        ));

        let mut trailing = good.clone();
        trailing.push(0);
        assert_eq!(
            decode(&trailing, identity(coord)),
            Err(CacheFormatError::TrailingBytes { extra: 1 })
        );

        let mut flipped = good.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 0xff;
        let error = rejected(&flipped, identity(coord), "a damaged payload");
        assert!(matches!(error, CacheFormatError::ChecksumMismatch { .. }));
        assert!(!error.is_stale());

        // A valid entry read under the wrong identity: the file is intact, so
        // the coordinate and the fingerprint are what reject it.
        let other_coord = ChunkCoord::new(9, 9, 9);
        let error = rejected(&good, identity(other_coord), "another chunk's entry");
        assert!(matches!(error, CacheFormatError::CoordMismatch { .. }));
        assert!(!error.is_stale());

        let other_identity = EntryIdentity {
            coord,
            source_fingerprint: 0xdead_beef_dead_beef,
        };
        let error = rejected(&good, other_identity, "another source identity");
        assert!(matches!(
            error,
            CacheFormatError::FingerprintMismatch { .. }
        ));
        assert!(error.is_stale());
    }

    #[test]
    fn absence_must_not_carry_a_payload_and_runs_must_be_positive() {
        let coord = ChunkCoord::default();
        let mut absent_with_payload = vec![0_u8; HEADER_BYTES + 2];
        let header = sealed(
            identity(coord),
            &SourceChunk::KnownAbsent,
            PayloadEncoding::Raw,
        );
        absent_with_payload[..HEADER_BYTES].copy_from_slice(&header);
        put_u32(&mut absent_with_payload, OFFSET_PAYLOAD_LEN, 2);
        reseal(&mut absent_with_payload);
        assert_eq!(
            decode(&absent_with_payload, identity(coord)),
            Err(CacheFormatError::AbsentWithPayload { len: 2 })
        );

        let mut zero_run = sealed(
            identity(coord),
            &SourceChunk::Present(Chunk::empty()),
            PayloadEncoding::Rle,
        );
        put_u32(&mut zero_run, HEADER_BYTES, 0);
        reseal(&mut zero_run);
        assert_eq!(
            decode(&zero_run, identity(coord)),
            Err(CacheFormatError::EmptyRun)
        );
    }

    #[test]
    fn a_run_payload_that_does_not_fill_the_chunk_is_rejected() {
        let coord = ChunkCoord::default();
        let mut short_runs = sealed(
            identity(coord),
            &SourceChunk::Present(Chunk::empty()),
            PayloadEncoding::Rle,
        );
        put_u32(&mut short_runs, HEADER_BYTES, 4);
        reseal(&mut short_runs);
        assert!(matches!(
            decode(&short_runs, identity(coord)),
            Err(CacheFormatError::PayloadCellCount { .. })
        ));
    }

    #[test]
    fn malformed_raw_and_rle_lengths_are_bounded_typed_errors() {
        let coord = ChunkCoord::new(i32::MIN, i32::MAX, -17);

        let mut short_raw = sealed(
            identity(coord),
            &SourceChunk::Present(Chunk::empty()),
            PayloadEncoding::Raw,
        );
        short_raw.pop();
        put_u32(
            &mut short_raw,
            OFFSET_PAYLOAD_LEN,
            (Chunk::BYTES - 1) as u32,
        );
        reseal(&mut short_raw);
        assert!(matches!(
            decode(&short_raw, identity(coord)),
            Err(CacheFormatError::PayloadCellCount { .. })
        ));

        let mut partial_run = sealed(
            identity(coord),
            &SourceChunk::Present(Chunk::empty()),
            PayloadEncoding::Rle,
        );
        partial_run.push(0xaa);
        put_u32(&mut partial_run, OFFSET_PAYLOAD_LEN, 7);
        reseal(&mut partial_run);
        assert!(matches!(
            decode(&partial_run, identity(coord)),
            Err(CacheFormatError::PayloadCellCount { .. })
        ));

        for hostile_count in [Chunk::VOLUME as u32 + 1, u32::MAX] {
            let mut overrun = sealed(
                identity(coord),
                &SourceChunk::Present(Chunk::empty()),
                PayloadEncoding::Rle,
            );
            put_u32(&mut overrun, HEADER_BYTES, hostile_count);
            reseal(&mut overrun);
            assert!(matches!(
                decode(&overrun, identity(coord)),
                Err(CacheFormatError::PayloadCellCount { .. })
            ));
        }
    }

    #[test]
    fn arbitrary_coordinate_bits_and_voxel_ids_have_defined_behavior() {
        let coord = ChunkCoord::new(-1, 2, -3);
        let mut other_coord = sealed(
            identity(coord),
            &SourceChunk::KnownAbsent,
            PayloadEncoding::Raw,
        );
        put_i32(&mut other_coord, OFFSET_COORD, i32::MAX);
        put_i32(&mut other_coord, OFFSET_COORD + 4, i32::MIN);
        put_i32(&mut other_coord, OFFSET_COORD + 8, -1);
        reseal(&mut other_coord);
        assert!(matches!(
            decode(&other_coord, identity(coord)),
            Err(CacheFormatError::CoordMismatch { .. })
        ));

        let mut all_max = sealed(
            identity(coord),
            &SourceChunk::Present(Chunk::empty()),
            PayloadEncoding::Rle,
        );
        all_max[HEADER_BYTES + 4..HEADER_BYTES + 6].copy_from_slice(&u16::MAX.to_le_bytes());
        reseal(&mut all_max);
        let decoded = decode(&all_max, identity(coord));
        match decoded {
            Ok(SourceChunk::Present(chunk)) => {
                assert_eq!(chunk.solid_count(), Chunk::VOLUME);
                assert_eq!(cell(&chunk, 0), VoxelId(u16::MAX));
                assert_eq!(cell(&chunk, Chunk::VOLUME - 1), VoxelId(u16::MAX));
            }
            other => panic!("valid arbitrary voxel id was not preserved: {other:?}"),
        }
    }

    /// Recomputes the checksum so a deliberately malformed body is tested for
    /// the property under test rather than for its stale checksum.
    fn reseal(bytes: &mut [u8]) {
        let mut digest_input = Vec::new();
        digest_input.extend_from_slice(&bytes[..OFFSET_CHECKSUM]);
        digest_input.extend_from_slice(&bytes[HEADER_BYTES..]);
        let value = checksum(&digest_input);
        put_u64(bytes, OFFSET_CHECKSUM, value);
    }

    #[test]
    fn the_run_length_encoding_is_smaller_on_the_diagnostic_fixture() {
        let source = crate::source::DiagnosticChunkSource;
        let SourceChunk::Present(chunk) = source.load(ChunkCoord::new(0, 0, 0)) else {
            panic!("the diagnostic corridor has content at the origin");
        };
        let (raw, rle) = match (
            encode_payload(&chunk, PayloadEncoding::Raw),
            encode_payload(&chunk, PayloadEncoding::Rle),
        ) {
            (Ok(raw), Ok(rle)) => (raw, rle),
            other => panic!("encoding the corridor chunk failed: {other:?}"),
        };
        assert_eq!(raw.len(), Chunk::BYTES);
        assert!(
            rle.len() < raw.len(),
            "rle {} is not smaller than raw {}",
            rle.len(),
            raw.len()
        );
    }

    #[test]
    fn run_length_worst_case_is_exact_and_bounded() {
        let mut alternating = Chunk::empty();
        for index in 0..Chunk::VOLUME {
            let value = if index % 2 == 0 {
                VoxelId(1)
            } else {
                VoxelId(2)
            };
            if let Err(error) = write_cell(&mut alternating, index, value) {
                panic!("alternating fixture write failed at {index}: {error}");
            }
        }
        let raw = encode_payload(&alternating, PayloadEncoding::Raw);
        let rle = encode_payload(&alternating, PayloadEncoding::Rle);
        match (raw, rle) {
            (Ok(raw), Ok(rle)) => {
                assert_eq!(raw.len(), Chunk::BYTES);
                assert_eq!(rle.len(), Chunk::VOLUME * RLE_RUN_BYTES);
                assert_eq!(rle.len(), raw.len() * 3);
                assert_eq!(HEADER_BYTES + rle.len(), MAX_ENTRY_BYTES);
            }
            other => panic!("worst-case fixture did not encode: {other:?}"),
        }
    }
}
