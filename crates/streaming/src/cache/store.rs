//! Filesystem placement and publication for cache entries.
//!
//! Entries have one logical value per key. The key is
//! `(format version, source fingerprint, chunk coord)`, so a new source
//! identity or format version writes to a different directory instead of
//! intentionally replacing content. Concurrent publishers may race: Windows
//! rename normally preserves the winner while Unix rename may atomically
//! replace it. Both bytes represent the same source result by contract, so
//! correctness depends on semantic equivalence, not cross-platform
//! "write-once" filesystem behavior.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use veldwake_voxel::ChunkCoord;

use super::format::{FORMAT_VERSION, MAX_ENTRY_BYTES};

/// Extension of a published entry. Only this extension is ever read.
const ENTRY_EXTENSION: &str = "vwc";
/// Prefix of an unpublished temporary file. Never read as an entry, and
/// removed on open.
const TEMP_PREFIX: &str = "tmp-";

/// Distinguishes temporary files written by concurrent publishers in the same
/// process. Combined with the process id it is unique on one host.
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// What publishing an entry did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishOutcome {
    /// The entry was written and published under its final name.
    Written,
    /// An entry already existed at the preflight check or won a rename race on
    /// a platform whose rename does not replace an existing target.
    AlreadyPresent,
}

/// Where cache entries live on disk.
#[derive(Clone, Debug)]
pub(crate) struct CacheStore {
    /// Directory holding every entry of one format version and source
    /// identity. The caller's root is never written to directly.
    entries: PathBuf,
    #[cfg(test)]
    reject_removals: bool,
}

impl CacheStore {
    /// Builds the store layout for one source identity. Creates nothing yet.
    pub(crate) fn new(root: &Path, source_fingerprint: u64) -> Self {
        Self {
            entries: root
                .join(format!("v{FORMAT_VERSION}"))
                .join(format!("{source_fingerprint:016x}")),
            #[cfg(test)]
            reject_removals: false,
        }
    }

    /// Narrow failure injection for the cache-policy recovery regression. It
    /// is test-only so filesystem behavior is not abstracted in production.
    #[cfg(test)]
    pub(crate) fn reject_removals(&mut self) {
        self.reject_removals = true;
    }

    pub(crate) fn entries_dir(&self) -> &Path {
        &self.entries
    }

    /// Deterministic file name for a chunk.
    ///
    /// Negative components are written as their two's-complement bit pattern
    /// in fixed-width hexadecimal, so every name is the same length, contains
    /// only `[0-9a-z_]`, and can never produce `..`, a separator, or any other
    /// traversal. The name is derived only from the coordinate; no external
    /// string ever reaches the path.
    pub(crate) fn entry_name(coord: ChunkCoord) -> String {
        format!(
            "x{:08x}_y{:08x}_z{:08x}.{ENTRY_EXTENSION}",
            coord.x.cast_unsigned(),
            coord.y.cast_unsigned(),
            coord.z.cast_unsigned()
        )
    }

    pub(crate) fn entry_path(&self, coord: ChunkCoord) -> PathBuf {
        self.entries.join(Self::entry_name(coord))
    }

    /// Reads an entry. `Ok(None)` means the file is absent, which is a cache
    /// miss and never authoritative absence.
    pub(crate) fn read(&self, coord: ChunkCoord) -> io::Result<Option<Vec<u8>>> {
        match File::open(self.entry_path(coord)) {
            Ok(file) => {
                // Read one byte beyond the largest legal entry. The decoder
                // will reject that bounded prefix as malformed, after which
                // policy removes the hostile file. `fs::read` would allocate
                // according to an attacker-controlled file length first.
                let limit = u64::try_from(MAX_ENTRY_BYTES)
                    .map_err(|_| io::Error::other("cache entry bound does not fit u64"))?
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("cache entry read bound overflow"))?;
                let mut reader = file.take(limit);
                // Let the buffer grow with the actual entry. Reserving the
                // worst case here would turn every tiny RLE/absence hit into
                // a ~192 KiB allocation even though `take` already bounds it.
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes)?;
                Ok(Some(bytes))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Publishes an entry without exposing the temporary write under the final
    /// name: bytes are written beside the target and then renamed. Same-volume
    /// rename is atomic for the targeted filesystems during normal operation.
    /// If concurrent publishers race, Windows commonly keeps the first target
    /// while Unix may atomically replace it; callers must provide equivalent
    /// bytes for one logical key and must not rely on which publisher wins.
    ///
    /// The temporary file is not flushed to stable storage before the rename.
    /// That is deliberate: this is a discardable cache whose entries carry a
    /// checksum, so a torn file after a power loss is detected on read and
    /// falls back to the source. Durability would cost an `fsync` per chunk
    /// and buy nothing a regeneration does not already give.
    pub(crate) fn publish(&self, coord: ChunkCoord, bytes: &[u8]) -> io::Result<PublishOutcome> {
        let path = self.entry_path(coord);
        if path.exists() {
            return Ok(PublishOutcome::AlreadyPresent);
        }
        fs::create_dir_all(&self.entries)?;

        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = self.entries.join(format!(
            "{TEMP_PREFIX}{:x}-{sequence:x}-{}",
            process::id(),
            Self::entry_name(coord)
        ));
        if let Err(error) = write_all(&temp, bytes) {
            remove_quietly(&temp);
            return Err(error);
        }
        match fs::rename(&temp, &path) {
            Ok(()) => Ok(PublishOutcome::Written),
            Err(error) => {
                remove_quietly(&temp);
                // Another publisher may have won on a non-replacing rename.
                // The logical value is equivalent, so this is not a failure.
                if path.exists() {
                    Ok(PublishOutcome::AlreadyPresent)
                } else {
                    Err(error)
                }
            }
        }
    }

    /// Removes an entry that was read and rejected, so the source result can
    /// republish it. This is the only reason a published entry is deleted.
    pub(crate) fn remove_entry(&self, coord: ChunkCoord) -> io::Result<bool> {
        #[cfg(test)]
        if self.reject_removals {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected rejected-entry deletion failure",
            ));
        }
        match fs::remove_file(self.entry_path(coord)) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Deletes temporary files left by a crashed or killed writer. Returns how
    /// many were removed so the residue is reported rather than hidden.
    pub(crate) fn sweep_temporaries(&self) -> io::Result<u64> {
        let listing = match fs::read_dir(&self.entries) {
            Ok(listing) => listing,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error),
        };
        let mut removed = 0;
        for entry in listing {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.starts_with(TEMP_PREFIX) {
                fs::remove_file(entry.path())?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Counts published entries and the bytes they occupy. Walked on demand
    /// rather than tracked, because this experiment has no eviction policy to
    /// keep a running total honest.
    pub(crate) fn footprint(&self) -> io::Result<CacheFootprint> {
        let listing = match fs::read_dir(&self.entries) {
            Ok(listing) => listing,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(CacheFootprint::default());
            }
            Err(error) => return Err(error),
        };
        let mut footprint = CacheFootprint::default();
        for entry in listing {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !name.ends_with(ENTRY_EXTENSION) || name.starts_with(TEMP_PREFIX) {
                continue;
            }
            footprint.entries += 1;
            footprint.bytes += entry.metadata()?.len();
        }
        Ok(footprint)
    }
}

/// How much disk one source identity's cache occupies.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheFootprint {
    pub entries: u64,
    pub bytes: u64,
}

fn write_all(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)
}

/// Best-effort cleanup of a temporary file on a failed publish. A leftover
/// temporary is swept on the next open and is never readable as an entry, so
/// failing to remove it now changes nothing.
fn remove_quietly(path: &Path) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != io::ErrorKind::NotFound
    {
        // Reported through the sweep counter on the next open rather than
        // through a per-chunk log line.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn published(store: &CacheStore, coord: ChunkCoord, bytes: &[u8]) -> PublishOutcome {
        match store.publish(coord, bytes) {
            Ok(outcome) => outcome,
            Err(error) => panic!("publish failed: {error}"),
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "veldwake-store-test-{}-{name}-{}",
            process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn entry_names_are_fixed_width_and_traversal_free() {
        for coord in [
            ChunkCoord::new(0, 0, 0),
            ChunkCoord::new(-1, -1, -1),
            ChunkCoord::new(i32::MIN, i32::MAX, -4),
        ] {
            let name = CacheStore::entry_name(coord);
            assert_eq!(name.len(), "x00000000_y00000000_z00000000.vwc".len());
            assert!(!name.contains(".."));
            assert!(!name.contains('/') && !name.contains('\\'));
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || matches!(c, '_' | '.')),
                "{name} has an unexpected character"
            );
        }
        assert_ne!(
            CacheStore::entry_name(ChunkCoord::new(-1, 0, 0)),
            CacheStore::entry_name(ChunkCoord::new(1, 0, 0)),
            "sign must survive the name"
        );
    }

    #[test]
    fn sequential_publication_preserves_an_existing_entry() {
        let root = temp_root("existing-entry");
        let store = CacheStore::new(&root, 0xabcd);
        let coord = ChunkCoord::new(-2, 0, 3);
        assert_eq!(store.read(coord).ok().flatten(), None, "missing is a miss");

        assert_eq!(published(&store, coord, b"first"), PublishOutcome::Written);
        assert_eq!(
            store.read(coord).ok().flatten().as_deref(),
            Some(&b"first"[..])
        );
        assert_eq!(
            published(&store, coord, b"second"),
            PublishOutcome::AlreadyPresent,
            "the preflight preserves an entry already visible to this publisher"
        );
        assert_eq!(
            store.read(coord).ok().flatten().as_deref(),
            Some(&b"first"[..])
        );

        let footprint = match store.footprint() {
            Ok(footprint) => footprint,
            Err(error) => panic!("footprint failed: {error}"),
        };
        assert_eq!(footprint.entries, 1);
        assert_eq!(footprint.bytes, 5);

        assert_eq!(store.remove_entry(coord).ok(), Some(true));
        assert_eq!(store.read(coord).ok().flatten(), None);
        assert_eq!(store.remove_entry(coord).ok(), Some(false));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn different_fingerprints_never_share_a_file() {
        let root = temp_root("fingerprints");
        let coord = ChunkCoord::new(1, 1, 1);
        let first = CacheStore::new(&root, 1);
        let second = CacheStore::new(&root, 2);
        assert_ne!(first.entry_path(coord), second.entry_path(coord));
        assert_eq!(published(&first, coord, b"a"), PublishOutcome::Written);
        assert_eq!(published(&second, coord, b"b"), PublishOutcome::Written);
        assert_eq!(first.read(coord).ok().flatten().as_deref(), Some(&b"a"[..]));
        assert_eq!(
            second.read(coord).ok().flatten().as_deref(),
            Some(&b"b"[..])
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn concurrent_publishers_leave_one_complete_value() {
        let root = temp_root("concurrent");
        let store = CacheStore::new(&root, 9);
        let coord = ChunkCoord::new(3, -2, 1);
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for bytes in [&b"publisher-a"[..], &b"publisher-b"[..]] {
            let store = store.clone();
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                store.publish(coord, bytes)
            }));
        }
        barrier.wait();

        let mut written = 0;
        for thread in threads {
            match thread.join() {
                Ok(Ok(PublishOutcome::Written)) => written += 1,
                Ok(Ok(PublishOutcome::AlreadyPresent)) => {}
                Ok(Err(error)) => panic!("concurrent publish failed: {error}"),
                Err(_) => panic!("concurrent publisher panicked"),
            }
        }
        assert!(written >= 1);
        let final_bytes = store.read(coord).ok().flatten();
        assert!(matches!(
            final_bytes.as_deref(),
            Some(b"publisher-a") | Some(b"publisher-b")
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn leftover_temporary_files_are_swept_and_never_read() {
        let root = temp_root("residue");
        let store = CacheStore::new(&root, 7);
        let coord = ChunkCoord::new(0, 0, 0);
        if let Err(error) = fs::create_dir_all(store.entries_dir()) {
            panic!("could not create the entry directory: {error}");
        }
        let residue = store.entries_dir().join(format!(
            "{TEMP_PREFIX}dead-{}",
            CacheStore::entry_name(coord)
        ));
        if let Err(error) = fs::write(&residue, b"half written") {
            panic!("could not create the residue: {error}");
        }

        // A temporary file is not an entry: the coordinate still misses.
        assert_eq!(store.read(coord).ok().flatten(), None);
        let footprint = store.footprint().unwrap_or_default();
        assert_eq!(footprint.entries, 0, "residue must not count as an entry");

        assert_eq!(store.sweep_temporaries().ok(), Some(1));
        assert!(!residue.exists());
        assert_eq!(store.sweep_temporaries().ok(), Some(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sweeping_a_cache_that_was_never_written_is_not_an_error() {
        let root = temp_root("empty");
        let store = CacheStore::new(&root, 3);
        assert_eq!(store.sweep_temporaries().ok(), Some(0));
        assert_eq!(store.footprint().ok(), Some(CacheFootprint::default()));
        assert!(!root.exists(), "probing must not create the directory");
    }
}
