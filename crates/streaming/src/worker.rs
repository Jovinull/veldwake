use std::{
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use veldwake_voxel::{
    CHUNK_EDGE, COARSE_EDGE, ChunkCoord, Mesh, OwnedMeshingSnapshot,
    mesh_exposed_faces_from_snapshot,
};

use crate::{
    cache::{CacheLoadOutcome, ChunkCache},
    source::{DiagnosticChunkSource, SourceChunk},
    types::{MeshStamp, RequestToken},
};

pub(crate) enum WorkerJob {
    Load {
        coord: ChunkCoord,
        token: RequestToken,
    },
    Mesh(Box<MeshJob>),
}

pub(crate) struct MeshJob {
    pub(crate) stamp: MeshStamp,
    pub(crate) snapshot: MeshSnapshot,
}

/// Owned meshing input at the level the job was issued for.
pub(crate) enum MeshSnapshot {
    Fine(OwnedMeshingSnapshot<CHUNK_EDGE>),
    Coarse(OwnedMeshingSnapshot<COARSE_EDGE>),
}

impl MeshSnapshot {
    pub(crate) fn payload_bytes(&self) -> usize {
        match self {
            Self::Fine(snapshot) => snapshot.payload_bytes(),
            Self::Coarse(snapshot) => snapshot.payload_bytes(),
        }
    }

    fn mesh(&self) -> Mesh {
        match self {
            Self::Fine(snapshot) => mesh_exposed_faces_from_snapshot(snapshot),
            Self::Coarse(snapshot) => mesh_exposed_faces_from_snapshot(snapshot),
        }
    }
}

#[derive(Debug)]
pub(crate) enum WorkerResult {
    Load {
        coord: ChunkCoord,
        token: RequestToken,
        source: SourceChunk,
        /// What the disk cache did for this load. All zero when no cache is
        /// configured, which is what keeps the cacheless path unchanged.
        cache: CacheLoadOutcome,
    },
    Mesh(Box<MeshResult>),
}

#[derive(Debug)]
pub(crate) struct MeshResult {
    pub(crate) stamp: MeshStamp,
    pub(crate) mesh: Mesh,
    /// Wall time the worker spent inside the mesher for this job.
    pub(crate) mesh_time: Duration,
}

pub(crate) struct Worker {
    jobs: Option<SyncSender<WorkerJob>>,
    results: Receiver<WorkerResult>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the worker thread. A cache passed here has already completed its
    /// cold open; every later lookup and publication happens on this thread,
    /// never in the caller's frame path.
    pub(crate) fn spawn(
        source: DiagnosticChunkSource,
        cache: Option<ChunkCache>,
    ) -> std::io::Result<Self> {
        let (job_tx, job_rx) = mpsc::sync_channel::<WorkerJob>(1);
        let (result_tx, result_rx) = mpsc::sync_channel::<WorkerResult>(1);
        let thread = thread::Builder::new()
            .name("veldwake-streaming".to_owned())
            .spawn(move || worker_loop(source, cache.as_ref(), &job_rx, &result_tx));
        let thread = thread?;
        Ok(Self {
            jobs: Some(job_tx),
            results: result_rx,
            thread: Some(thread),
        })
    }

    pub(crate) fn try_dispatch(&self, job: WorkerJob) -> Result<(), WorkerJob> {
        let Some(jobs) = &self.jobs else {
            return Err(job);
        };
        match jobs.try_send(job) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(job) | TrySendError::Disconnected(job)) => Err(job),
        }
    }

    pub(crate) fn try_result(&self) -> Result<Option<WorkerResult>, ()> {
        match self.results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(()),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            eprintln!("veldwake streaming worker panicked during shutdown");
        }
    }
}

fn worker_loop(
    source: DiagnosticChunkSource,
    cache: Option<&ChunkCache>,
    jobs: &Receiver<WorkerJob>,
    results: &SyncSender<WorkerResult>,
) {
    while let Ok(job) = jobs.recv() {
        let result = match job {
            WorkerJob::Load { coord, token } => {
                // Cache first, source second; the source always decides the
                // value the runtime receives, cached or not.
                let (loaded, cache_outcome) = match cache {
                    Some(cache) => cache.load_with(coord, || source.load(coord)),
                    None => (source.load(coord), CacheLoadOutcome::default()),
                };
                WorkerResult::Load {
                    coord,
                    token,
                    source: loaded,
                    cache: cache_outcome,
                }
            }
            WorkerJob::Mesh(job) => {
                let started = Instant::now();
                let mesh = job.snapshot.mesh();
                WorkerResult::Mesh(Box::new(MeshResult {
                    stamp: job.stamp,
                    mesh,
                    mesh_time: started.elapsed(),
                }))
            }
        };
        if results.send(result).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_shutdown_is_clean_with_and_without_work() {
        let worker = Worker::spawn(DiagnosticChunkSource, None);
        assert!(worker.is_ok());
        drop(worker);

        let worker = match Worker::spawn(DiagnosticChunkSource, None) {
            Ok(worker) => worker,
            Err(error) => panic!("test worker failed to start: {error}"),
        };
        let result = worker.try_dispatch(WorkerJob::Load {
            coord: ChunkCoord::default(),
            token: RequestToken::FIRST,
        });
        assert!(result.is_ok());
        drop(worker);
    }
}
