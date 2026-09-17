//! Experimental disk cache for chunk source results.
//!
//! **This is a cache, not a save.** Every entry is reproducible by asking the
//! source again, so any entry may be deleted at any time without losing
//! information. Nothing here is authoritative world state, nothing here
//! records a player edit, and nothing here promises a migration path. The
//! authoritative persistence that a world-editing milestone will need is a
//! separate problem with separate guarantees; this experiment only proves the
//! boundary, the format discipline, and the cost.
//!
//! Per-chunk lookup, decode, source fallback, and publication sit inside the
//! streaming worker's load path. Opening is a separate cold-start operation:
//! it sweeps temporary files and walks the footprint on the caller's thread.
//! The client performs that opt-in setup during event-loop initialization,
//! never in the frame hot path. It learns no entry-format details.
//!
//! Policy, in one place:
//!
//! - a missing file is a **miss**, never authoritative absence;
//! - an entry that fails any check is discarded and the source answers;
//! - a rejected entry is deleted so the source result can republish it;
//! - a failed write never prevents a valid source result from reaching the
//!   runtime;
//! - `KnownAbsent` is stored explicitly as a typed entry with no payload.

mod format;
mod store;

use std::{io, path::PathBuf, time::Instant};

use veldwake_voxel::ChunkCoord;

use crate::source::SourceChunk;

pub use format::PayloadEncoding;
pub use store::CacheFootprint;

use format::EntryIdentity;
use store::{CacheStore, PublishOutcome};

/// Where the experiment stores entries and how it encodes payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheConfig {
    /// Root directory for the experiment. Never the repository working tree:
    /// the caller passes a diagnostic or temporary directory explicitly.
    root: PathBuf,
    payload_encoding: PayloadEncoding,
}

impl CacheConfig {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            payload_encoding: PayloadEncoding::default(),
        }
    }

    #[must_use]
    pub fn with_payload_encoding(mut self, encoding: PayloadEncoding) -> Self {
        self.payload_encoding = encoding;
        self
    }
}

/// What opening the cache found and cleaned up.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheOpenReport {
    pub entries_dir: PathBuf,
    /// Unpublished temporary files left by an earlier crash, now removed.
    pub temporaries_removed: u64,
    pub footprint: CacheFootprint,
}

/// Per-load cache accounting, produced on the worker thread and folded into
/// the runtime metrics when the result is integrated. Every field counts at
/// most one event, so the runtime only ever adds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CacheLoadOutcome {
    pub(crate) lookups: u64,
    pub(crate) hits_present: u64,
    pub(crate) hits_absent: u64,
    pub(crate) misses: u64,
    pub(crate) stale_rejects: u64,
    pub(crate) corrupt_rejects: u64,
    pub(crate) read_failures: u64,
    /// Rejected files successfully removed before best-effort publication.
    /// This does not by itself claim the replacement was published.
    pub(crate) rejected_entries_removed: u64,
    /// Rejected files that could not be removed. The source value is still
    /// returned, but the poisoned entry can cause another fallback later.
    pub(crate) rejected_entry_delete_failures: u64,
    pub(crate) source_fallbacks: u64,
    pub(crate) write_attempts: u64,
    pub(crate) writes: u64,
    pub(crate) writes_skipped: u64,
    pub(crate) write_failures: u64,
    pub(crate) bytes_read: u64,
    pub(crate) bytes_written: u64,
    pub(crate) decode_nanos: u64,
    pub(crate) encode_nanos: u64,
}

/// One source identity's cache.
#[derive(Clone, Debug)]
pub struct ChunkCache {
    store: CacheStore,
    source_fingerprint: u64,
    payload_encoding: PayloadEncoding,
}

impl ChunkCache {
    /// Opens the cache for one source identity, removing temporary files left
    /// by an earlier run before anything reads or writes.
    ///
    /// Opening creates no directory: a cache that is never written leaves no
    /// trace on disk.
    pub fn open(
        config: &CacheConfig,
        source_fingerprint: u64,
    ) -> io::Result<(Self, CacheOpenReport)> {
        let store = CacheStore::new(&config.root, source_fingerprint);
        let temporaries_removed = store.sweep_temporaries()?;
        let footprint = store.footprint()?;
        let report = CacheOpenReport {
            entries_dir: store.entries_dir().to_path_buf(),
            temporaries_removed,
            footprint,
        };
        Ok((
            Self {
                store,
                source_fingerprint,
                payload_encoding: config.payload_encoding,
            },
            report,
        ))
    }

    #[cfg(test)]
    pub(crate) fn entries_dir(&self) -> &std::path::Path {
        self.store.entries_dir()
    }

    /// Entries and bytes currently on disk for this source identity.
    ///
    /// This experiment has no eviction policy, so the footprint is walked on
    /// demand rather than tracked; an untracked running total would be a
    /// promise the experiment does not keep.
    pub fn footprint(&self) -> io::Result<CacheFootprint> {
        self.store.footprint()
    }

    /// Removes every entry of this source identity. The explicit way to reset
    /// the diagnostic cache, since nothing evicts automatically.
    pub fn clear(&self) -> io::Result<()> {
        match std::fs::remove_dir_all(self.store.entries_dir()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn identity(&self, coord: ChunkCoord) -> EntryIdentity {
        EntryIdentity {
            coord,
            source_fingerprint: self.source_fingerprint,
        }
    }

    /// Answers one load: the cache first, the source second.
    ///
    /// `source` is called only on a miss, a rejection, or a read failure, and
    /// its result is always what the caller receives. Publishing is best
    /// effort by construction: the value is returned whether or not it could
    /// be written, so a read-only or full disk degrades throughput and never
    /// correctness.
    pub(crate) fn load_with<F>(
        &self,
        coord: ChunkCoord,
        source: F,
    ) -> (SourceChunk, CacheLoadOutcome)
    where
        F: FnOnce() -> SourceChunk,
    {
        let mut outcome = CacheLoadOutcome {
            lookups: 1,
            ..CacheLoadOutcome::default()
        };

        match self.store.read(coord) {
            Ok(Some(bytes)) => {
                outcome.bytes_read = bytes.len() as u64;
                let started = Instant::now();
                let decoded = format::decode(&bytes, self.identity(coord));
                outcome.decode_nanos = elapsed_nanos(started);
                match decoded {
                    Ok(SourceChunk::Present(chunk)) => {
                        outcome.hits_present = 1;
                        return (SourceChunk::Present(chunk), outcome);
                    }
                    Ok(SourceChunk::KnownAbsent) => {
                        outcome.hits_absent = 1;
                        return (SourceChunk::KnownAbsent, outcome);
                    }
                    Err(error) => {
                        if error.is_stale() {
                            outcome.stale_rejects = 1;
                        } else {
                            outcome.corrupt_rejects = 1;
                        }
                        // Delete before best-effort republish. A cleanup
                        // failure must not lose the source result, but it must
                        // be observable: otherwise one poisoned entry can
                        // trigger a silent fallback forever.
                        match self.store.remove_entry(coord) {
                            Ok(true) => outcome.rejected_entries_removed = 1,
                            Ok(false) => {}
                            Err(_) => outcome.rejected_entry_delete_failures = 1,
                        }
                    }
                }
            }
            Ok(None) => outcome.misses = 1,
            Err(_) => outcome.read_failures = 1,
        }

        outcome.source_fallbacks = 1;
        let value = source();
        self.publish(coord, &value, &mut outcome);
        (value, outcome)
    }

    fn publish(&self, coord: ChunkCoord, value: &SourceChunk, outcome: &mut CacheLoadOutcome) {
        outcome.write_attempts = 1;
        let started = Instant::now();
        let encoded = format::encode(self.identity(coord), value, self.payload_encoding);
        outcome.encode_nanos = elapsed_nanos(started);
        let Ok(bytes) = encoded else {
            // Encoding a chunk the runtime already holds cannot fail, but a
            // failure is counted rather than assumed away.
            outcome.write_failures = 1;
            return;
        };
        match self.store.publish(coord, &bytes) {
            Ok(PublishOutcome::Written) => {
                outcome.writes = 1;
                outcome.bytes_written = bytes.len() as u64;
            }
            Ok(PublishOutcome::AlreadyPresent) => outcome.writes_skipped = 1,
            Err(_) => outcome.write_failures = 1,
        }
    }
}

/// Elapsed nanoseconds, saturating instead of wrapping on an absurd clock.
fn elapsed_nanos(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::DiagnosticChunkSource;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    use veldwake_voxel::{Chunk, VoxelId, fingerprint};

    static ROOTS: AtomicU64 = AtomicU64::new(0);

    fn temp_config(name: &str) -> CacheConfig {
        let root = std::env::temp_dir().join(format!(
            "veldwake-cache-test-{}-{name}-{}",
            std::process::id(),
            ROOTS.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        CacheConfig::new(root)
    }

    fn open(config: &CacheConfig, fingerprint: u64) -> (ChunkCache, CacheOpenReport) {
        match ChunkCache::open(config, fingerprint) {
            Ok(opened) => opened,
            Err(error) => panic!("cache failed to open: {error}"),
        }
    }

    #[test]
    fn a_cold_miss_consults_the_source_and_publishes_it() {
        let config = temp_config("cold");
        let (cache, report) = open(&config, 0x1111);
        assert_eq!(report.temporaries_removed, 0);
        assert_eq!(report.footprint, CacheFootprint::default());

        let coord = ChunkCoord::new(0, 0, 0);
        let (value, outcome) = cache.load_with(coord, || DiagnosticChunkSource.load(coord));
        assert!(matches!(value, SourceChunk::Present(_)));
        assert_eq!(outcome.lookups, 1);
        assert_eq!(outcome.misses, 1);
        assert_eq!(outcome.source_fallbacks, 1);
        assert_eq!(outcome.writes, 1);
        assert_eq!(outcome.hits_present, 0);
        assert!(outcome.bytes_written > 0);
        assert!(outcome.encode_nanos > 0);

        let footprint = cache.footprint().unwrap_or_default();
        assert_eq!(footprint.entries, 1);
        assert!(footprint.bytes > 0);
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn a_warm_hit_never_consults_the_source_and_returns_the_same_content() {
        let config = temp_config("warm");
        let (cache, _) = open(&config, 0x2222);
        let coord = ChunkCoord::new(-1, 0, 2);
        let (cold, _) = cache.load_with(coord, || DiagnosticChunkSource.load(coord));

        let (warm, outcome) = cache.load_with(coord, || {
            panic!("a warm hit must not consult the source");
        });
        assert_eq!(outcome.hits_present, 1);
        assert_eq!(outcome.source_fallbacks, 0);
        assert_eq!(outcome.write_attempts, 0);
        assert!(outcome.bytes_read > 0);
        match (&cold, &warm) {
            (SourceChunk::Present(cold), SourceChunk::Present(warm)) => {
                assert_eq!(fingerprint(cold), fingerprint(warm), "content must match");
                assert_eq!(cold.solid_count(), warm.solid_count());
            }
            other => panic!("expected two present chunks: {other:?}"),
        }
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn known_absence_is_stored_and_replayed_as_a_distinct_hit() {
        let config = temp_config("absent");
        let (cache, _) = open(&config, 0x3333);
        // Outside the finite corridor: authoritative absence, not a miss.
        let coord = ChunkCoord::new(99, 0, 0);
        let (cold, outcome) = cache.load_with(coord, || DiagnosticChunkSource.load(coord));
        assert_eq!(cold, SourceChunk::KnownAbsent);
        assert_eq!(outcome.misses, 1);
        assert_eq!(outcome.writes, 1);

        let (warm, outcome) = cache.load_with(coord, || {
            panic!("stored absence must not consult the source");
        });
        assert_eq!(warm, SourceChunk::KnownAbsent);
        assert_eq!(outcome.hits_absent, 1);
        assert_eq!(outcome.hits_present, 0, "absence is its own counter");
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn a_missing_file_is_a_miss_and_never_absence() {
        let config = temp_config("missing");
        let (cache, _) = open(&config, 0x4444);
        // A coordinate the source answers with content; if a missing file were
        // read as absence, this would come back absent.
        let coord = ChunkCoord::new(1, 0, 1);
        let (value, outcome) = cache.load_with(coord, || DiagnosticChunkSource.load(coord));
        assert_eq!(outcome.misses, 1);
        assert_eq!(outcome.hits_absent, 0);
        assert!(matches!(value, SourceChunk::Present(_)));
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn a_different_source_fingerprint_never_reads_the_old_entry() {
        let config = temp_config("fingerprint");
        let coord = ChunkCoord::new(2, 0, -2);
        let (old, _) = open(&config, 0x5555);
        let (_, outcome) = old.load_with(coord, || DiagnosticChunkSource.load(coord));
        assert_eq!(outcome.writes, 1);

        let (new, _) = open(&config, 0x6666);
        let mut consulted = false;
        let (_, outcome) = new.load_with(coord, || {
            consulted = true;
            DiagnosticChunkSource.load(coord)
        });
        assert!(consulted, "a new source identity must re-ask the source");
        assert_eq!(outcome.misses, 1, "a new identity uses a new key");
        assert_eq!(outcome.stale_rejects, 0);

        // The old identity's entry is untouched, not silently reused.
        assert_eq!(old.footprint().unwrap_or_default().entries, 1);
        assert_eq!(new.footprint().unwrap_or_default().entries, 1);
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn a_corrupt_entry_falls_back_to_the_source_and_is_replaced() {
        let config = temp_config("corrupt");
        let (cache, _) = open(&config, 0x7777);
        let coord = ChunkCoord::new(0, 0, -1);
        let (_, outcome) = cache.load_with(coord, || DiagnosticChunkSource.load(coord));
        assert_eq!(outcome.writes, 1);

        let path = cache.entries_dir().join(CacheStore::entry_name(coord));
        let mut bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => panic!("could not read the entry: {error}"),
        };
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        if let Err(error) = fs::write(&path, &bytes) {
            panic!("could not damage the entry: {error}");
        }

        let mut consulted = false;
        let (value, outcome) = cache.load_with(coord, || {
            consulted = true;
            DiagnosticChunkSource.load(coord)
        });
        assert!(consulted, "a corrupt entry must fall back to the source");
        assert_eq!(outcome.corrupt_rejects, 1);
        assert_eq!(outcome.stale_rejects, 0);
        assert_eq!(
            outcome.rejected_entries_removed, 1,
            "the rejected entry must be removed"
        );
        assert_eq!(outcome.rejected_entry_delete_failures, 0);
        assert_eq!(outcome.writes, 1, "the source result republishes it");
        assert!(matches!(value, SourceChunk::Present(_)));

        // The repaired entry now reads back cleanly.
        let (_, outcome) = cache.load_with(coord, || {
            panic!("the repaired entry must hit");
        });
        assert_eq!(outcome.hits_present, 1);
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn an_entry_written_for_another_chunk_is_rejected_as_corrupt() {
        let config = temp_config("swapped");
        let (cache, _) = open(&config, 0x8888);
        let first = ChunkCoord::new(1, 0, 0);
        let second = ChunkCoord::new(2, 0, 0);
        cache.load_with(first, || DiagnosticChunkSource.load(first));

        // Move the first entry onto the second coordinate's path: the header
        // still names the first chunk.
        let from = cache.entries_dir().join(CacheStore::entry_name(first));
        let to = cache.entries_dir().join(CacheStore::entry_name(second));
        if let Err(error) = fs::rename(&from, &to) {
            panic!("could not move the entry: {error}");
        }

        let mut consulted = false;
        let (_, outcome) = cache.load_with(second, || {
            consulted = true;
            DiagnosticChunkSource.load(second)
        });
        assert!(consulted);
        assert_eq!(outcome.corrupt_rejects, 1);
        assert_eq!(outcome.rejected_entries_removed, 1);
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn a_rejected_entry_delete_failure_is_visible_and_never_loses_the_source_result() {
        let config = temp_config("delete-failure");
        let (mut cache, _) = open(&config, 0x8989);
        let coord = ChunkCoord::new(0, 0, 0);
        cache.load_with(coord, || DiagnosticChunkSource.load(coord));

        let path = cache.entries_dir().join(CacheStore::entry_name(coord));
        let mut bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => panic!("could not read the entry: {error}"),
        };
        bytes[0] ^= 0xff;
        if let Err(error) = fs::write(&path, bytes) {
            panic!("could not corrupt the entry: {error}");
        }
        cache.store.reject_removals();

        for _ in 0..2 {
            let (value, outcome) = cache.load_with(coord, || DiagnosticChunkSource.load(coord));
            assert!(matches!(value, SourceChunk::Present(_)));
            assert_eq!(outcome.corrupt_rejects, 1);
            assert_eq!(outcome.rejected_entries_removed, 0);
            assert_eq!(outcome.rejected_entry_delete_failures, 1);
            assert_eq!(outcome.source_fallbacks, 1);
            assert_eq!(outcome.writes_skipped, 1);
            assert_eq!(outcome.writes, 0);
        }
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn an_oversized_entry_is_read_with_a_hard_bound_then_replaced() {
        let config = temp_config("oversized");
        let (cache, _) = open(&config, 0x8a8a);
        let coord = ChunkCoord::new(0, 0, 0);
        if let Err(error) = fs::create_dir_all(cache.entries_dir()) {
            panic!("could not create entry directory: {error}");
        }
        let path = cache.entries_dir().join(CacheStore::entry_name(coord));
        if let Err(error) = fs::write(&path, vec![0_u8; format::MAX_ENTRY_BYTES + 4096]) {
            panic!("could not create oversized entry: {error}");
        }

        let (value, outcome) = cache.load_with(coord, || DiagnosticChunkSource.load(coord));
        assert!(matches!(value, SourceChunk::Present(_)));
        assert_eq!(outcome.bytes_read, (format::MAX_ENTRY_BYTES + 1) as u64);
        assert_eq!(outcome.corrupt_rejects, 1);
        assert_eq!(outcome.rejected_entries_removed, 1);
        assert_eq!(outcome.writes, 1);
        let repaired_len = fs::metadata(&path).map(|metadata| metadata.len());
        assert!(matches!(repaired_len, Ok(len) if len <= format::MAX_ENTRY_BYTES as u64));
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn a_write_failure_still_returns_the_source_result() {
        let config = temp_config("write-failure");
        let (cache, _) = open(&config, 0x9999);
        let coord = ChunkCoord::new(0, 0, 0);
        // A plain file where the entry directory must be: creating the
        // directory fails, so publishing cannot succeed.
        if let Err(error) =
            fs::create_dir_all(config.root.join(format!("v{}", format::FORMAT_VERSION)))
        {
            panic!("could not prepare the root: {error}");
        }
        if let Err(error) = fs::write(cache.entries_dir(), b"not a directory") {
            panic!("could not block the entry directory: {error}");
        }

        let (value, outcome) = cache.load_with(coord, || DiagnosticChunkSource.load(coord));
        assert!(
            matches!(value, SourceChunk::Present(_)),
            "a failed write must not lose the source result"
        );
        assert_eq!(outcome.write_attempts, 1);
        assert_eq!(outcome.write_failures, 1);
        assert_eq!(outcome.writes, 0);
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn leftover_temporaries_are_reported_when_the_cache_opens() {
        let config = temp_config("residue");
        let (cache, _) = open(&config, 0xaaaa);
        let coord = ChunkCoord::new(0, 0, 0);
        cache.load_with(coord, || DiagnosticChunkSource.load(coord));
        let residue = cache
            .entries_dir()
            .join("tmp-dead-x00000000_y00000000_z00000000.vwc");
        if let Err(error) = fs::write(&residue, b"partial") {
            panic!("could not create the residue: {error}");
        }

        let (_, report) = open(&config, 0xaaaa);
        assert_eq!(report.temporaries_removed, 1);
        assert_eq!(report.footprint.entries, 1, "the real entry survives");
        assert!(!residue.exists());
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn both_payload_encodings_return_identical_content() {
        let coord = ChunkCoord::new(-3, 0, 1);
        let mut fingerprints = Vec::new();
        let mut sizes = Vec::new();
        for encoding in [PayloadEncoding::Raw, PayloadEncoding::Rle] {
            let config = temp_config(encoding.name()).with_payload_encoding(encoding);
            let (cache, _) = open(&config, 0xbbbb);
            cache.load_with(coord, || DiagnosticChunkSource.load(coord));
            let (warm, outcome) = cache.load_with(coord, || {
                panic!("the warm pass must hit");
            });
            assert_eq!(outcome.hits_present, 1);
            match warm {
                SourceChunk::Present(chunk) => fingerprints.push(fingerprint(&chunk)),
                other => panic!("expected content: {other:?}"),
            }
            sizes.push(cache.footprint().unwrap_or_default().bytes);
            let _ = fs::remove_dir_all(&config.root);
        }
        assert_eq!(fingerprints[0], fingerprints[1], "encodings must agree");
        assert!(
            sizes[1] < sizes[0],
            "run-length {} should beat raw {} on this fixture",
            sizes[1],
            sizes[0]
        );
    }

    #[test]
    fn encoding_preference_only_affects_new_entries() {
        let coord = ChunkCoord::new(-3, 0, 1);
        for (written, reopened) in [
            (PayloadEncoding::Raw, PayloadEncoding::Rle),
            (PayloadEncoding::Rle, PayloadEncoding::Raw),
        ] {
            let config = temp_config("encoding-preference").with_payload_encoding(written);
            let (cache, _) = open(&config, 0xbcbc);
            cache.load_with(coord, || DiagnosticChunkSource.load(coord));
            let before = cache.footprint().unwrap_or_default();

            let reopened_config = config.clone().with_payload_encoding(reopened);
            let (cache, _) = open(&reopened_config, 0xbcbc);
            let (_, outcome) = cache.load_with(coord, || {
                panic!("either supported on-disk encoding must remain readable")
            });
            assert_eq!(outcome.hits_present, 1, "written={written:?}");
            assert_eq!(outcome.write_attempts, 0);
            assert_eq!(cache.footprint().unwrap_or_default(), before);
            let _ = fs::remove_dir_all(&config.root);
        }
    }

    #[test]
    fn clearing_removes_every_entry_of_that_identity() {
        let config = temp_config("clear");
        let (cache, _) = open(&config, 0xcccc);
        for x in -2..=2 {
            let coord = ChunkCoord::new(x, 0, 0);
            cache.load_with(coord, || DiagnosticChunkSource.load(coord));
        }
        assert_eq!(cache.footprint().unwrap_or_default().entries, 5);
        if let Err(error) = cache.clear() {
            panic!("clear failed: {error}");
        }
        assert_eq!(
            cache.footprint().unwrap_or_default(),
            CacheFootprint::default()
        );
        // Clearing twice is not an error.
        assert!(cache.clear().is_ok());
        let _ = fs::remove_dir_all(&config.root);
    }

    #[test]
    fn a_chunk_with_every_material_round_trips_through_disk() {
        let config = temp_config("materials");
        let (cache, _) = open(&config, 0xdddd);
        let coord = ChunkCoord::new(-4, -1, 4);
        let mut chunk = Chunk::empty();
        for (index, id) in [(0_usize, 1_u16), (1, 2), (1000, 7), (32767, u16::MAX)] {
            let x = index % 32;
            let y = (index / 32) % 32;
            let z = index / 1024;
            if let Err(error) = chunk.write(x, y, z, VoxelId(id)) {
                panic!("fixture write failed: {error}");
            }
        }
        let expected = fingerprint(&chunk);
        let solids = chunk.solid_count();

        cache.load_with(coord, || SourceChunk::Present(chunk.clone()));
        let (warm, outcome) = cache.load_with(coord, || panic!("must hit"));
        assert_eq!(outcome.hits_present, 1);
        match warm {
            SourceChunk::Present(warm) => {
                assert_eq!(fingerprint(&warm), expected);
                assert_eq!(warm.solid_count(), solids);
            }
            other => panic!("expected content: {other:?}"),
        }
        let _ = fs::remove_dir_all(&config.root);
    }
}
