use std::{
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    thread::{self, JoinHandle},
};

use veldwake_voxel::{
    CHUNK_EDGE, COARSE_EDGE, ChunkCoord, Mesh, OwnedMeshingSnapshot,
    mesh_exposed_faces_from_snapshot,
};

use crate::{
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
    },
    Mesh(Box<MeshResult>),
}

#[derive(Debug)]
pub(crate) struct MeshResult {
    pub(crate) stamp: MeshStamp,
    pub(crate) mesh: Mesh,
}

pub(crate) struct Worker {
    jobs: Option<SyncSender<WorkerJob>>,
    results: Receiver<WorkerResult>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub(crate) fn spawn(source: DiagnosticChunkSource) -> std::io::Result<Self> {
        let (job_tx, job_rx) = mpsc::sync_channel::<WorkerJob>(1);
        let (result_tx, result_rx) = mpsc::sync_channel::<WorkerResult>(1);
        let thread = thread::Builder::new()
            .name("veldwake-streaming".to_owned())
            .spawn(move || worker_loop(source, &job_rx, &result_tx));
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
    jobs: &Receiver<WorkerJob>,
    results: &SyncSender<WorkerResult>,
) {
    while let Ok(job) = jobs.recv() {
        let result = match job {
            WorkerJob::Load { coord, token } => WorkerResult::Load {
                coord,
                token,
                source: source.load(coord),
            },
            WorkerJob::Mesh(job) => WorkerResult::Mesh(Box::new(MeshResult {
                stamp: job.stamp,
                mesh: job.snapshot.mesh(),
            })),
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
        let worker = Worker::spawn(DiagnosticChunkSource);
        assert!(worker.is_ok());
        drop(worker);

        let worker = match Worker::spawn(DiagnosticChunkSource) {
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
