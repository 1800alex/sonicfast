//! Background downloads and filesystem-derived availability. No UI-thread I/O.
use l::playback::Cancellation;
use libresonic::{
    self as l, AudioStream, MediaId, MusicService, ServiceFuture, StreamProvider, Track,
    opensubsonic::Client,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    task::JoinSet,
};
const MAX_JOBS: usize = 10_000;
const MAX_FILE: u64 = 8 * 1024 * 1024 * 1024;
const MAX_INDEX: u64 = 32 * 1024 * 1024;
const COLLISION_SLOTS: usize = 8;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub path: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Remote,
    Queued,
    Active,
    Retrying,
    Paused,
    Cancelled,
    Complete,
    Missing,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub track: Track,
    pub status: Status,
    pub bytes: u64,
    pub total: Option<u64>,
    pub relative: Option<PathBuf>,
    pub sha256: Option<String>,
    pub modified_ns: Option<u64>,
    pub attempts: u8,
    pub retry_at: u64,
    pub error: Option<String>,
}
impl Job {
    fn new(track: Track) -> Self {
        Self {
            total: track.file_size,
            track,
            status: Status::Queued,
            bytes: 0,
            relative: None,
            sha256: None,
            modified_ns: None,
            attempts: 0,
            retry_at: 0,
            error: None,
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Memory {
    version: u32,
    paused: bool,
    jobs: Vec<Job>,
}
/// Unfinished work saved by the app inside its existing playback resume state.
/// Completed-file availability is deliberately absent.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Pending {
    paused: bool,
    jobs: Vec<Job>,
}
fn pending(status: &Status) -> bool {
    matches!(
        status,
        Status::Queued
            | Status::Active
            | Status::Retrying
            | Status::Paused
            | Status::Cancelled
            | Status::Failed
    )
}
fn reserved(job: &Job) -> bool {
    matches!(
        job.status,
        Status::Queued | Status::Active | Status::Retrying | Status::Paused | Status::Complete
    )
}
fn make_room(memory: &mut Memory) {
    if memory.jobs.len() >= 2 * MAX_JOBS
        && let Some(at) = memory.jobs.iter().position(|job| !pending(&job.status))
    {
        memory.jobs.remove(at);
    }
}
#[derive(Clone)]
pub enum Update {
    Reset(Vec<Job>, bool),
    Job(Box<Job>),
    Notice(String),
    Paused(bool),
}
type Notify = Arc<dyn Fn(Update) + Send + Sync>;
#[derive(Clone)]
pub struct Library {
    root: PathBuf,
    index: Arc<Mutex<Memory>>,
    known: Arc<Mutex<HashMap<MediaId, Track>>>,
    notify: Notify,
    enabled: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
fn modified(meta: &std::fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos()
        .try_into()
        .ok()
}
fn hash_id(id: &MediaId) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(id).expect("media ID serializes"))
    )
}
fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn io_error(_: std::io::Error) -> String {
    "Unable to access the download folder or file; check permissions and free space".into()
}
fn component(value: &str, fallback: &str) -> String {
    component_for(value, fallback, cfg!(windows))
}
fn portable_component(value: &str, fallback: &str) -> String {
    component_for(value, fallback, true)
}
fn component_for(value: &str, fallback: &str, windows: bool) -> String {
    let mut result = String::new();
    for c in value.trim().chars() {
        if result.len() + c.len_utf8() > 120 {
            break;
        }
        result.push(
            if c.is_control() || "/\\".contains(c) || (windows && "<>:\"|?*".contains(c)) {
                '_'
            } else {
                c
            },
        );
    }
    let result = result.trim_matches([' ', '.']).to_owned();
    let result = if result.is_empty() {
        fallback.to_owned()
    } else {
        result
    };
    let stem = result.split('.').next().unwrap_or("").to_ascii_uppercase();
    if windows
        && matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    {
        format!("_{result}")
    } else {
        result
    }
}
fn server_path(value: &str) -> Option<PathBuf> {
    let parts: Vec<_> = value.split(['/', '\\']).collect();
    if !(1..=32).contains(&parts.len())
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
        || parts[0].eq_ignore_ascii_case(".sonicfast")
        || (parts[0].as_bytes().get(1) == Some(&b':')
            && parts[0].as_bytes()[0].is_ascii_alphabetic())
    {
        return None;
    }
    Some(parts.into_iter().map(|p| component(p, "track")).collect())
}
fn collision_path(path: &Path, identity: &str) -> PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("track");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("audio");
    path.with_file_name(format!("{stem} -- {identity}.{ext}"))
}
fn collision_paths(natural: &Path, id: &MediaId) -> Vec<PathBuf> {
    let hash = hash_id(id);
    (0..COLLISION_SLOTS)
        .map(|slot| {
            let identity = if slot == 0 {
                hash[..12].to_string()
            } else {
                format!("{}-{slot}", &hash[..12])
            };
            collision_path(natural, &identity)
        })
        .collect()
}
/// Exact candidate paths only; this never enumerates a music directory.
pub fn candidates(track: &Track) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = track.file_path.as_deref().and_then(server_path) {
        paths.push(path);
    }
    paths.extend(metadata_candidates(track, component));
    if let Some(path) = paths.first().cloned() {
        paths.extend(collision_paths(&path, &track.id));
    }
    // Keep the previous portable names discoverable after the naming correction.
    paths.extend(metadata_candidates(track, portable_component));
    let mut unique = Vec::new();
    for path in paths {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    unique
}
fn metadata_candidates(track: &Track, component: fn(&str, &str) -> String) -> Vec<PathBuf> {
    let artist = component(
        track
            .artists
            .first()
            .map_or("Unknown artist", |a| a.name.as_str()),
        "Unknown artist",
    );
    let album = component(
        track
            .album
            .as_ref()
            .map_or("Unknown album", |a| a.title.as_str()),
        "Unknown album",
    );
    let folder = PathBuf::from(artist).join(album);
    let suffix = component(track.suffix.as_deref().unwrap_or("audio"), "audio");
    let title = component(&track.title, "Untitled track");
    let mut names = Vec::new();
    if let Some(file) = track
        .file_path
        .as_deref()
        .and_then(|p| p.rsplit(['/', '\\']).next())
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
    {
        names.push(component(file, "track"));
    }
    if let Some(n) = track.track_number {
        names.push(format!("{n:02} - {title}.{suffix}"));
    }
    names.push(format!("{title}.{suffix}"));
    names.dedup();
    let mut paths: Vec<_> = names.into_iter().map(|n| folder.join(n)).collect();
    // The server may correct a virtual tag-based path to the original disk
    // filename. Preserve deterministic older title/numbered-title names, too;
    // their identity suffix must not depend on today's advertised basename.
    for natural in paths.clone() {
        paths.extend(collision_paths(&natural, &track.id));
    }
    paths
}
async fn destination(library: &Library, track: &Track) -> Result<(PathBuf, PathBuf), String> {
    let natural = candidates(track)
        .into_iter()
        .next()
        .ok_or("Unable to name this download")?;
    for relative in std::iter::once(natural.clone()).chain(collision_paths(&natural, &track.id)) {
        let target = safe_path(&library.root, &relative, true).await?;
        if tokio::fs::try_exists(&target).await.map_err(io_error)? {
            continue;
        }
        let mut index = lock(&library.index);
        if index.jobs.iter().any(|job| {
            job.track.id != track.id && reserved(job) && job.relative.as_ref() == Some(&relative)
        }) {
            continue;
        }
        // Reserve the natural filename before another concurrent transfer selects it.
        if let Some(job) = index.jobs.iter_mut().find(|job| job.track.id == track.id) {
            job.relative = Some(relative.clone());
        }
        return Ok((relative, target));
    }
    Err("Unable to choose an unused download filename; retry this item".into())
}
async fn safe_path(root: &Path, relative: &Path, create: bool) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Invalid relative download path".into());
    }
    let mut current = root.to_path_buf();
    let parts: Vec<_> = relative.components().collect();
    for (i, part) in parts.iter().enumerate() {
        current.push(part.as_os_str());
        if create && i + 1 < parts.len() {
            match tokio::fs::create_dir(&current).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io_error(e)),
            }
        }
        match tokio::fs::symlink_metadata(&current).await {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Download paths cannot contain symbolic links".into());
            }
            Ok(meta) if i + 1 < parts.len() && !meta.is_dir() => {
                return Err("A download folder is not a directory".into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_error(e)),
        }
    }
    Ok(current)
}
async fn digest_file(path: &Path) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path).await.map_err(io_error)?;
    let mut digest = Sha256::new();
    let mut bytes = vec![0; 64 * 1024];
    loop {
        let n = file.read(&mut bytes).await.map_err(io_error)?;
        if n == 0 {
            break;
        }
        digest.update(&bytes[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
impl Library {
    pub fn register(&self, tracks: &[Track]) {
        let mut known = lock(&self.known);
        for track in tracks {
            if known.len() >= MAX_JOBS
                && !known.contains_key(&track.id)
                && let Some(old) = known.keys().next().cloned()
            {
                known.remove(&old);
            }
            known.insert(track.id.clone(), track.clone());
        }
    }
    fn update(&self, job: Job) {
        {
            let mut memory = lock(&self.index);
            if let Some(old) = memory
                .jobs
                .iter_mut()
                .find(|old| old.track.id == job.track.id)
            {
                if old.status != job.status && (pending(&old.status) || pending(&job.status)) {
                    self.dirty.store(true, Ordering::Release);
                }
                *old = job.clone();
            } else {
                make_room(&mut memory);
                if pending(&job.status) {
                    self.dirty.store(true, Ordering::Release);
                }
                memory.jobs.push(job.clone());
            }
        }
        (self.notify)(Update::Job(Box::new(job)));
    }
    pub async fn lookup(&self, id: &MediaId) -> Option<PathBuf> {
        if !self.enabled.load(Ordering::Acquire) {
            return None;
        }
        let existing = lock(&self.index)
            .jobs
            .iter()
            .find(|job| &job.track.id == id)
            .cloned();
        let track = lock(&self.known)
            .get(id)
            .cloned()
            .or_else(|| existing.as_ref().map(|job| job.track.clone()))?;
        let size = track
            .file_size
            .filter(|n| *n > 0 && *n <= MAX_FILE)
            .or_else(|| {
                existing
                    .as_ref()
                    .filter(|job| job.bytes > 0)
                    .map(|job| job.bytes)
            });
        let mut paths = candidates(&track);
        if let Some(relative) = existing.as_ref().and_then(|job| job.relative.clone()) {
            paths.retain(|path| path != &relative);
            paths.insert(0, relative);
        }
        for relative in paths {
            if lock(&self.index).jobs.iter().any(|job| {
                job.track.id != *id && reserved(job) && job.relative.as_ref() == Some(&relative)
            }) {
                continue;
            }
            let Ok(path) = safe_path(&self.root, &relative, false).await else {
                continue;
            };
            let Ok(meta) = tokio::fs::metadata(&path).await else {
                continue;
            };
            if !meta.is_file()
                || meta.len() == 0
                || meta.len() > MAX_FILE
                || size != Some(meta.len())
            {
                continue;
            }
            let cached = existing.as_ref().filter(|job| {
                job.relative.as_ref() == Some(&relative)
                    && job.bytes == meta.len()
                    && job.modified_ns == modified(&meta)
            });
            let digest = match cached.and_then(|job| job.sha256.clone()) {
                Some(hash) => hash,
                None => match digest_file(&path).await {
                    Ok(hash) => hash,
                    Err(_) => continue,
                },
            };
            // During this session, retain integrity evidence for a known original.
            // Restoring that original or moving it to another exact candidate is valid.
            if existing
                .as_ref()
                .and_then(|job| job.sha256.as_ref())
                .is_some_and(|hash| hash != &digest)
            {
                continue;
            }
            let mut job = Job::new(track.clone());
            job.status = Status::Complete;
            job.bytes = meta.len();
            job.total = Some(meta.len());
            job.relative = Some(relative);
            job.sha256 = Some(digest);
            job.modified_ns = modified(&meta);
            {
                let mut memory = lock(&self.index);
                if memory
                    .jobs
                    .iter()
                    .any(|old| old.track.id != *id && reserved(old) && old.relative == job.relative)
                {
                    continue;
                }
                if let Some(old) = memory.jobs.iter_mut().find(|old| old.track.id == *id) {
                    if pending(&old.status) {
                        self.dirty.store(true, Ordering::Release);
                    }
                    *old = job.clone();
                } else {
                    make_room(&mut memory);
                    memory.jobs.push(job.clone());
                }
            }
            (self.notify)(Update::Job(Box::new(job)));
            return Some(path);
        }
        // Re-read under the lock: a transfer/queue command can advance while probing.
        let mut memory = lock(&self.index);
        let mut job = memory
            .jobs
            .iter()
            .find(|job| &job.track.id == id)
            .cloned()
            .unwrap_or_else(|| Job::new(track));
        if !pending(&job.status)
            || existing.is_none() && !memory.jobs.iter().any(|job| &job.track.id == id)
        {
            job.status = Status::Remote;
            job.error = None;
        }
        if let Some(old) = memory.jobs.iter_mut().find(|old| &old.track.id == id) {
            *old = job.clone();
        } else {
            make_room(&mut memory);
            memory.jobs.push(job.clone());
        }
        (self.notify)(Update::Job(Box::new(job)));
        None
    }
}
pub struct LocalFirst {
    pub client: Arc<Client>,
    pub library: Arc<Mutex<Option<Library>>>,
}
impl StreamProvider for LocalFirst {
    fn open<'a>(&'a self, id: &'a MediaId) -> ServiceFuture<'a, Box<dyn AudioStream>> {
        Box::pin(async move {
            if id.server() != &self.client.capabilities().server {
                return Err(l::Error::WrongServer);
            }
            let library = { lock(&self.library).clone() };
            if let Some(library) = library
                && let Some(path) = library.lookup(id).await
                && let Ok(file) = tokio::fs::File::open(path).await
                && let Ok(meta) = file.metadata().await
            {
                return Ok(Box::new(LocalStream {
                    file,
                    length: meta.len(),
                }) as Box<dyn AudioStream>);
            }
            self.client.open(id).await
        })
    }
}
struct LocalStream {
    file: tokio::fs::File,
    length: u64,
}
impl AudioStream for LocalStream {
    fn byte_len(&self) -> Option<u64> {
        Some(self.length)
    }
    fn is_seekable(&self) -> bool {
        true
    }
    fn seek_to(&mut self, offset: u64) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            if offset > self.length {
                return Err(l::Error::InvalidResponse);
            }
            self.file
                .seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|_| l::Error::Network)?;
            Ok(())
        })
    }
    fn next(&mut self) -> ServiceFuture<'_, Option<Vec<u8>>> {
        Box::pin(async move {
            let mut bytes = vec![0; l::STREAM_CHUNK_BYTES];
            let n = self
                .file
                .read(&mut bytes)
                .await
                .map_err(|_| l::Error::Network)?;
            bytes.truncate(n);
            Ok((n > 0).then_some(bytes))
        })
    }
}
struct Active {
    task_id: Option<tokio::task::Id>,
    cancel: Cancellation,
    after: Status,
}
struct Finished {
    job: Job,
    retry: Option<u64>,
}
pub struct Manager {
    pub library: Library,
    client: Arc<Client>,
    _lease: std::fs::File,
    tasks: JoinSet<Finished>,
    active: HashMap<MediaId, Active>,
    probe: JoinSet<()>,
    probe_pending: std::collections::VecDeque<Track>,
    server: l::ServerId,
}
impl Manager {
    pub async fn new(root: PathBuf, client: Arc<Client>, notify: Notify) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("Choose an absolute download folder path".into());
        }
        tokio::fs::create_dir_all(&root).await.map_err(io_error)?;
        let root = tokio::fs::canonicalize(&root).await.map_err(io_error)?;
        let lease_path = safe_path(&root, Path::new(".sonicfast/downloads.lock"), true).await?;
        let lease = tokio::task::spawn_blocking(move || {
            let file = std::fs::File::options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(lease_path)
                .map_err(io_error)?;
            file.try_lock().map_err(|_| {
                "Another Sonicfast instance is using this download folder".to_string()
            })?;
            Ok::<_, String>(file)
        })
        .await
        .map_err(|_| "Unable to lock the download folder".to_owned())??;
        // The old catalog is an optional, read-only hint. Bad/missing JSON cannot
        // prevent downloads or local discovery, and this file is never rewritten.
        let mut index = Memory {
            version: 1,
            ..Memory::default()
        };
        if let Ok(path) = safe_path(&root, Path::new(".sonicfast/index.json"), false).await
            && let Ok(meta) = tokio::fs::metadata(&path).await
        {
            let legacy = if meta.len() <= MAX_INDEX {
                tokio::fs::read(path)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Memory>(&bytes).ok())
            } else {
                None
            };
            if let Some(mut legacy) = legacy.filter(|m| {
                m.version == 1 && m.jobs.len() <= MAX_JOBS && m.jobs.iter().all(valid_job)
            }) {
                for job in &mut legacy.jobs {
                    if !pending(&job.status) {
                        job.status = Status::Remote;
                        job.sha256 = None;
                        job.modified_ns = None;
                    }
                    if job.status == Status::Active {
                        job.status = Status::Queued;
                        job.bytes = 0;
                        job.attempts = job.attempts.saturating_sub(1);
                    }
                }
                index = legacy;
            } else {
                notify(Update::Notice("The legacy download index was ignored; availability will be checked from files".into()));
            }
        }
        let server = client.capabilities().server;
        let library = Library {
            root,
            index: Arc::new(Mutex::new(index)),
            known: Arc::new(Mutex::new(HashMap::new())),
            notify,
            enabled: Arc::new(AtomicBool::new(true)),
            dirty: Arc::new(AtomicBool::new(true)),
        };
        let initial = lock(&library.index).clone();
        library.register(
            &initial
                .jobs
                .iter()
                .filter(|j| j.track.id.server() == &server)
                .map(|j| j.track.clone())
                .collect::<Vec<_>>(),
        );
        (library.notify)(Update::Reset(
            initial
                .jobs
                .into_iter()
                .filter(|j| j.track.id.server() == &server)
                .collect(),
            initial.paused,
        ));
        Ok(Self {
            library,
            client,
            _lease: lease,
            tasks: JoinSet::new(),
            active: HashMap::new(),
            probe: JoinSet::new(),
            probe_pending: std::collections::VecDeque::new(),
            server,
        })
    }
    pub fn publish(&self) {
        let index = lock(&self.library.index);
        (self.library.notify)(Update::Reset(
            index
                .jobs
                .iter()
                .filter(|j| j.track.id.server() == &self.server)
                .cloned()
                .collect(),
            index.paused,
        ));
    }
    pub fn enqueue(&mut self, tracks: Vec<Track>) {
        self.library.register(&tracks);
        self.library.dirty.store(true, Ordering::Release);
        let mut index = lock(&self.library.index);
        for track in tracks {
            if track.id.server() != &self.server || track.id.kind() != l::MediaKind::Track {
                continue;
            }
            if index.jobs.iter().filter(|job| pending(&job.status)).count() >= MAX_JOBS
                && !index
                    .jobs
                    .iter()
                    .any(|job| job.track.id == track.id && pending(&job.status))
            {
                (self.library.notify)(Update::Notice(
                    "The download queue is full (10,000 tracks)".into(),
                ));
                break;
            }
            if let Some(job) = index.jobs.iter_mut().find(|j| j.track.id == track.id) {
                if matches!(
                    job.status,
                    Status::Queued | Status::Active | Status::Retrying
                ) {
                    continue;
                }
                job.track = track;
                job.status = Status::Queued;
                job.error = None;
                job.attempts = 0;
                job.retry_at = 0;
                (self.library.notify)(Update::Job(Box::new(job.clone())));
            } else if index.jobs.iter().filter(|job| pending(&job.status)).count() < MAX_JOBS {
                make_room(&mut index);
                let job = Job::new(track);
                (self.library.notify)(Update::Job(Box::new(job.clone())));
                index.jobs.push(job);
            } else {
                (self.library.notify)(Update::Notice(
                    "The download queue is full (10,000 tracks)".into(),
                ));
                break;
            }
        }
    }
    pub fn probe(&mut self, tracks: Vec<Track>) {
        self.library.register(&tracks);
        for track in tracks {
            let active = lock(&self.library.index)
                .jobs
                .iter()
                .find(|job| job.track.id == track.id && job.status == Status::Active)
                .cloned();
            if let Some(job) = active {
                (self.library.notify)(Update::Job(Box::new(job)));
                continue;
            }
            if self.probe_pending.len() < 2000 {
                self.probe_pending.push_back(track);
            } else {
                let mut job = Job::new(track);
                job.status = Status::Remote;
                (self.library.notify)(Update::Job(Box::new(job)));
            }
        }
    }
    pub fn control(&mut self, id: Option<&MediaId>, action: &str) {
        self.library.dirty.store(true, Ordering::Release);
        let mut index = lock(&self.library.index);
        if id.is_none() {
            index.paused = action == "pause";
            (self.library.notify)(Update::Paused(index.paused));
        }
        for job in index.jobs.iter_mut().filter(|j| {
            j.track.id.server() == &self.server && id.is_none_or(|id| &j.track.id == id)
        }) {
            if action == "first" && id.is_some() {
                continue;
            }
            if id.is_none()
                && !matches!(
                    job.status,
                    Status::Queued | Status::Active | Status::Retrying
                )
            {
                continue;
            }

            if let Some(active) = self.active.get_mut(&job.track.id) {
                active.after = if action == "cancel" {
                    Status::Cancelled
                } else {
                    Status::Queued
                };
                active.cancel.cancel();
            } else if !matches!(job.status, Status::Complete | Status::Remote) {
                job.status = if action == "cancel" {
                    Status::Cancelled
                } else {
                    Status::Queued
                };
                if action == "retry" {
                    job.attempts = 0;
                    job.retry_at = 0;
                    job.error = None;
                }
                (self.library.notify)(Update::Job(Box::new(job.clone())));
            }
        }
        if action == "first"
            && let Some(id) = id
            && let Some(at) = index.jobs.iter().position(|j| &j.track.id == id)
        {
            let job = index.jobs.remove(at);
            index.jobs.insert(0, job);
            (self.library.notify)(Update::Reset(
                index
                    .jobs
                    .iter()
                    .filter(|j| j.track.id.server() == &self.server)
                    .cloned()
                    .collect(),
                index.paused,
            ));
        }
    }
    pub fn root(&self) -> &Path {
        &self.library.root
    }
    pub fn pending(&self) -> Pending {
        let memory = lock(&self.library.index);
        Pending {
            paused: memory.paused,
            jobs: memory
                .jobs
                .iter()
                .filter(|job| pending(&job.status) && job.track.id.server() == &self.server)
                .cloned()
                .map(|mut job| {
                    if job.status == Status::Active {
                        job.status = self
                            .active
                            .get(&job.track.id)
                            .filter(|active| active.cancel.is_cancelled())
                            .map_or(Status::Queued, |active| active.after.clone());
                        job.bytes = 0;
                        job.attempts = job.attempts.saturating_sub(1);
                    }
                    job
                })
                .collect(),
        }
    }
    pub fn take_pending(&self) -> Option<Pending> {
        self.library
            .dirty
            .swap(false, Ordering::AcqRel)
            .then(|| self.pending())
    }
    pub fn restore(&mut self, state: Pending) -> Result<(), String> {
        if state.jobs.len() > MAX_JOBS
            || state.jobs.iter().any(|job| {
                !valid_job(job) || job.track.id.server() != &self.server || !pending(&job.status)
            })
        {
            return Err("The saved download queue is invalid".into());
        }
        let tracks: Vec<_> = state.jobs.iter().map(|job| job.track.clone()).collect();
        {
            let mut memory = lock(&self.library.index);
            memory.jobs.retain(|job| !pending(&job.status));
            for mut job in state.jobs {
                if job.status == Status::Active {
                    job.status = Status::Queued;
                    job.bytes = 0;
                }
                memory.jobs.retain(|old| old.track.id != job.track.id);
                memory.jobs.push(job);
            }
            memory.paused = state.paused;
        }
        self.library.register(&tracks);
        self.library.dirty.store(true, Ordering::Release);
        self.publish();
        Ok(())
    }
    pub async fn tick(&mut self) {
        while let Some(result) = self.tasks.try_join_next_with_id() {
            if let Err(error) = &result
                && let Some(id) = self.active.iter().find_map(|(id, active)| {
                    (active.task_id == Some(error.id())).then(|| id.clone())
                })
            {
                self.active.remove(&id);
                let job = lock(&self.library.index)
                    .jobs
                    .iter()
                    .find(|job| job.track.id == id)
                    .cloned();
                if let Some(mut job) = job {
                    job.status = Status::Failed;
                    job.error =
                        Some("The download task stopped unexpectedly; retry this item".into());
                    self.library.update(job);
                }
            }
            if let Ok((_, mut finished)) = result {
                if let Some(active) = self.active.remove(&finished.job.track.id)
                    && active.cancel.is_cancelled()
                    && finished.job.status != Status::Complete
                {
                    finished.job.status = active.after;
                    finished.job.attempts = finished.job.attempts.saturating_sub(1);
                    finished.job.error = None;
                    finished.retry = None;
                }
                if let Some(delay) = finished.retry
                    && finished.job.attempts < 3
                {
                    finished.job.status = Status::Retrying;
                    finished.job.retry_at = now()
                        .saturating_add(delay.saturating_mul(
                            1u64 << finished.job.attempts.saturating_sub(1).min(2),
                        ));
                }
                self.library.update(finished.job);
            }
        }
        while self.probe.try_join_next().is_some() {}
        while self.probe.len() < 2 && !self.probe_pending.is_empty() {
            let batch: Vec<_> = self
                .probe_pending
                .drain(..self.probe_pending.len().min(64))
                .collect();
            let library = self.library.clone();
            self.probe.spawn(async move {
                for track in batch {
                    library.lookup(&track.id).await;
                }
            });
        }
        if lock(&self.library.index).paused {
            return;
        }
        while self.active.len() < 2 {
            let job = {
                let index = lock(&self.library.index);
                index
                    .jobs
                    .iter()
                    .find(|j| {
                        j.track.id.server() == &self.server
                            && !self.active.contains_key(&j.track.id)
                            && (j.status == Status::Queued
                                || (j.status == Status::Retrying && j.retry_at <= now()))
                    })
                    .cloned()
            };
            let Some(mut job) = job else {
                break;
            };
            job.status = Status::Active;
            job.attempts = job.attempts.saturating_add(1);
            job.bytes = 0;
            job.error = None;
            let cancel = Cancellation::default();
            self.active.insert(
                job.track.id.clone(),
                Active {
                    task_id: None,
                    cancel: cancel.clone(),
                    after: Status::Queued,
                },
            );
            self.library.update(job.clone());
            let client = self.client.clone();
            let library = self.library.clone();
            let id = job.track.id.clone();
            let task = self
                .tasks
                .spawn(async move { run_download(client, library, job, cancel).await });
            if let Some(active) = self.active.get_mut(&id) {
                active.task_id = Some(task.id());
            }
        }
    }
    pub async fn shutdown(mut self) {
        self.library.enabled.store(false, Ordering::Release);
        self.probe.abort_all();
        while self.probe.join_next().await.is_some() {}
        for active in self.active.values() {
            active.cancel.cancel();
        }
        while let Some(result) = self.tasks.join_next().await {
            if let Ok(mut result) = result {
                if result.job.status != Status::Complete {
                    result.job.status = self
                        .active
                        .get(&result.job.track.id)
                        .map_or(Status::Queued, |active| active.after.clone());
                    result.job.attempts = result.job.attempts.saturating_sub(1);
                }
                result.job.error = None;
                self.library.update(result.job);
            }
        }
    }
}
fn valid_job(job: &Job) -> bool {
    job.track.id.kind() == l::MediaKind::Track
        && job.relative.as_ref().is_none_or(|path| {
            path.components().next().is_some_and(|part| {
                !part
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(".sonicfast")
            })
        })
        && job.relative.as_ref().is_none_or(|path| {
            !path.as_os_str().is_empty()
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
        })
}
struct Part(PathBuf);
impl Drop for Part {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
async fn run_download(
    client: Arc<Client>,
    library: Library,
    mut job: Job,
    cancel: Cancellation,
) -> Finished {
    let mut retry = None;
    let result=async {
        let track=tokio::select! {_=cancel.cancelled()=>return Err("Download cancelled".into()),track=client.song(&job.track.id)=>track.map_err(|e|{retry=retry_delay(&e);e.to_string()})?};
        job.track=track;job.total=job.track.file_size;
        if job.total.is_some_and(|n|n>MAX_FILE) {return Err("Original file exceeds the 8 GiB per-file limit".into());}
        library.register(std::slice::from_ref(&job.track));
        let existing=tokio::select! {_=cancel.cancelled()=>return Err("Download cancelled".into()),path=library.lookup(&job.track.id)=>path};
        if existing.is_some() {return Ok(());}
        let (relative,target)=destination(&library,&job.track).await?;
        job.relative=Some(relative.clone());
        let part_relative=PathBuf::from(".sonicfast/partials").join(format!("{}.part",hash_id(&job.track.id)));
        let part_path=safe_path(&library.root,&part_relative,true).await?;
        // This internal partial namespace is exclusively held by the folder lease.
        if tokio::fs::try_exists(&part_path).await.map_err(io_error)? {tokio::fs::remove_file(&part_path).await.map_err(io_error)?;}
        let mut file=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&part_path).await.map_err(io_error)?;
        let part=Part(part_path);
        let mut stream=tokio::select! {_=cancel.cancelled()=>return Err("Download cancelled".into()),stream=client.download(&job.track.id)=>stream.map_err(|e|{retry=retry_delay(&e);e.to_string()})?};
        if let (Some(size),Some(length))=(job.total,stream.byte_len()) && size!=length {return Err("The original file size changed; refresh and retry".into());}
        job.total=stream.byte_len().or(job.total);
        if job.total.is_some_and(|n|n>MAX_FILE) {return Err("Original file exceeds the 8 GiB per-file limit".into());}
        let mut digest=Sha256::new();let mut published=Instant::now();
        loop {
            let next=tokio::select! {_=cancel.cancelled()=>return Err("Download cancelled".into()),next=stream.next()=>next.map_err(|e|{retry=retry_delay(&e);e.to_string()})?};
            let Some(bytes)=next else {break;};
            job.bytes=job.bytes.saturating_add(bytes.len() as u64);
            if bytes.len()>l::STREAM_CHUNK_BYTES || job.bytes>MAX_FILE || job.total.is_some_and(|n|job.bytes>n) {return Err("The download exceeded its expected size".into());}
            file.write_all(&bytes).await.map_err(io_error)?;digest.update(&bytes);
            if published.elapsed()>=Duration::from_millis(250) {library.update(job.clone());published=Instant::now();}
        }
        if job.bytes==0 || job.total.is_some_and(|n|n!=job.bytes) {return Err("The download ended before the original file was complete".into());}
        file.flush().await.map_err(io_error)?;file.sync_all().await.map_err(io_error)?;drop(file);
        if cancel.is_cancelled() {return Err("Download cancelled".into());}
        // Atomic, same-filesystem publication that cannot replace an existing file.
        tokio::fs::hard_link(&part.0,&target).await.map_err(|_|"Unable to publish this download without overwriting a file; the folder must support hard links".to_string())?;
        job.sha256=Some(format!("{:x}",digest.finalize()));job.relative=Some(relative);job.modified_ns=modified(&tokio::fs::metadata(target).await.map_err(io_error)?);job.status=Status::Complete;
        library.update(job.clone());drop(part);Ok(())
    }.await;
    if let Err(error) = result {
        job.status = Status::Failed;
        job.error = Some(error);
    } else if job.status != Status::Complete
        && let Some(existing) = lock(&library.index)
            .jobs
            .iter()
            .find(|j| j.track.id == job.track.id && j.status == Status::Complete)
            .cloned()
    {
        job = existing;
    }
    Finished { job, retry }
}
fn retry_delay(error: &l::Error) -> Option<u64> {
    match error {
        l::Error::Network | l::Error::Timeout => Some(2000),
        l::Error::Http { status: 500..=599 } => Some(5000),
        l::Error::RateLimited { retry_after } => Some(
            retry_after
                .unwrap_or(Duration::from_secs(5))
                .as_millis()
                .clamp(1000, u128::from(u64::MAX)) as u64,
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsonic::test_server::{Reply, Server};
    fn root(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/download-tests")
            .join(format!("{name}-{:016x}", rand::random::<u64>()))
    }
    fn song(id: &str, size: u64) -> Track {
        Track {
            id: MediaId::new(
                l::ServerId::new("fixture").unwrap(),
                l::MediaKind::Track,
                id,
            )
            .unwrap(),
            title: "Same title".into(),
            artists: vec![l::ArtistRef {
                id: None,
                name: "Artist".into(),
            }],
            album: Some(l::AlbumRef {
                id: None,
                title: "Album".into(),
            }),
            duration_ms: Some(1000),
            track_number: Some(1),
            disc_number: None,
            content_type: Some("audio/flac".into()),
            suffix: Some("flac".into()),
            artwork: None,
            favorite: false,
            rating: None,
            file_path: Some("Artist/Album/original.flac".into()),
            file_size: Some(size),
            play_count: None,
        }
    }
    async fn manager(root: PathBuf, client: Arc<Client>) -> Manager {
        Manager::new(root, client, Arc::new(|_| {})).await.unwrap()
    }
    async fn finish(manager: &mut Manager, want: Status) {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            manager.tick().await;
            if lock(&manager.library.index)
                .jobs
                .iter()
                .any(|j| j.status == want)
            {
                return;
            }
            assert!(Instant::now() < until, "download did not reach {want:?}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    async fn server(bytes: Vec<u8>, stall: bool) -> Server {
        Server::start(move |request| {
            if request.path=="/rest/getSong.view" {Reply::json(serde_json::json!({"status":"ok","song":{"id":request.params["id"],"title":"Same title","artist":"Artist","album":"Album","path":"Artist/Album/original.flac","suffix":"flac","size":bytes.len()}}))}
            else if request.path=="/rest/download.view" {let mut reply=Reply::audio(bytes.clone());reply.stall=stall;reply}
            else {let mut reply=Reply::audio(vec![]);reply.status=500;reply}
        }).await
    }
    #[tokio::test]
    async fn filesystem_add_delete_restore_and_move_work_without_an_index() {
        let server = server(vec![1; 4096], false).await;
        let root = self::root("filesystem-truth");
        let m = manager(root.clone(), server.client()).await;
        let track = song("a", 4096);
        m.library.register(std::slice::from_ref(&track));
        assert!(m.library.lookup(&track.id).await.is_none());
        let path = safe_path(&root, &candidates(&track)[0], true)
            .await
            .unwrap();
        tokio::fs::write(&path, vec![1; 4096]).await.unwrap();
        assert_eq!(m.library.lookup(&track.id).await, Some(path.clone()));
        assert!(
            m.pending().jobs.is_empty(),
            "availability is never part of the saved queue"
        );
        tokio::fs::remove_file(&path).await.unwrap();
        assert!(m.library.lookup(&track.id).await.is_none());
        assert_eq!(lock(&m.library.index).jobs[0].status, Status::Remote);
        tokio::fs::write(&path, vec![1; 4096]).await.unwrap();
        assert_eq!(m.library.lookup(&track.id).await, Some(path.clone()));
        let moved = root.join(candidates(&track)[1].clone());
        tokio::fs::rename(&path, &moved).await.unwrap();
        assert_eq!(m.library.lookup(&track.id).await, Some(moved.clone()));
        tokio::fs::write(&moved, vec![1; 20]).await.unwrap();
        assert!(m.library.lookup(&track.id).await.is_none());
        tokio::fs::write(&moved, vec![1; 4096]).await.unwrap();
        m.shutdown().await;
        assert!(!root.join(".sonicfast/index.json").exists());
        let m = manager(root.clone(), server.client()).await;
        m.library.register(std::slice::from_ref(&track));
        assert_eq!(m.library.lookup(&track.id).await, Some(moved));
        assert_eq!(
            server.requests.lock().unwrap().len(),
            0,
            "file discovery makes no HTTP requests"
        );
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn original_disk_layout_and_older_tag_names_are_discovered_from_wire_metadata() {
        const ORIGINAL: &str = "Bad Astronaut/Houston We Have a Drinking Problem/Bad Astronaut - Another Dead Romance.m4a";
        const VIRTUAL: &str =
            "Bad Astronaut/Houston: We Have a Drinking Problem/Another Dead Romance.m4a";
        let bytes = vec![42; 4096];
        let server = Server::start(|request| {
            if request.path == "/rest/download.view" {
                return Reply::audio(vec![42; 4096]);
            }
            assert_eq!(request.path, "/rest/getSong.view");
            Reply::json(serde_json::json!({"status":"ok", "song": {
                "id":"a", "title":"Another Dead Romance", "artist":"Bad Astronaut",
                "album":"Houston: We Have a Drinking Problem", "suffix":"m4a",
                "path":ORIGINAL, "size":4096
            }}))
        })
        .await;
        let client = server.client();
        let neutral = client.song(&song("a", 4096).id).await.unwrap();
        // The same round trip as a visible row's ProbeDownloads action.
        let track =
            super::super::adapter::neutral_track(&super::super::adapter::track(&neutral)).unwrap();
        assert_eq!(track.file_path.as_deref(), Some(ORIGINAL));
        let hash = hash_id(&track.id);
        let old_portable = VIRTUAL.replace(':', "_");
        let mut layouts = vec![
            server_path(ORIGINAL).unwrap(),
            server_path(VIRTUAL).unwrap(),
            collision_path(&server_path(VIRTUAL).unwrap(), &hash[..12]),
            collision_path(&server_path(&old_portable).unwrap(), &hash[..12]),
        ];
        // A local library can also store songs directly in its root.
        let mut flat = track.clone();
        flat.file_path = Some("Bad Astronaut - Another Dead Romance.m4a".into());
        layouts.push(PathBuf::from(flat.file_path.as_ref().unwrap()));
        for (i, relative) in layouts.into_iter().enumerate() {
            let track = if i == 4 { flat.clone() } else { track.clone() };
            assert!(
                candidates(&track).contains(&relative),
                "missing {relative:?}"
            );
            let root = self::root("original-path");
            tokio::fs::create_dir_all(&root).await.unwrap();
            let path = safe_path(&root, &relative, true).await.unwrap();
            tokio::fs::write(&path, &bytes).await.unwrap();
            let updates = Arc::new(Mutex::new(Vec::new()));
            let notify = updates.clone();
            let mut m = Manager::new(
                root.clone(),
                client.clone(),
                Arc::new(move |u| {
                    lock(&notify).push(u);
                }),
            )
            .await
            .unwrap();
            m.probe(vec![track.clone()]);
            finish(&mut m, Status::Complete).await;
            assert_eq!(m.library.lookup(&track.id).await, Some(path.clone()));
            assert!(
                lock(&updates).iter().any(|u| matches!(u,
                Update::Job(j) if j.track.id == track.id && j.status == Status::Complete)),
                "the UI receives the local status for {relative:?}"
            );
            assert!(!root.join(".sonicfast/index.json").exists());
            let provider = LocalFirst {
                client: client.clone(),
                library: Arc::new(Mutex::new(Some(m.library.clone()))),
            };
            let mut stream = provider.open(&track.id).await.unwrap();
            assert_eq!(stream.next().await.unwrap().unwrap(), bytes);
            drop(stream);
            m.shutdown().await;
            // No remembered catalog or path is needed after restart.
            let m = manager(root.clone(), client.clone()).await;
            m.library.register(std::slice::from_ref(&track));
            assert_eq!(m.library.lookup(&track.id).await, Some(path));
            m.shutdown().await;
            tokio::fs::remove_dir_all(root).await.unwrap();
        }
        assert_eq!(server.count("/rest/download.view"), 0);
        assert_eq!(server.count("/rest/stream.view"), 0);
        let root = self::root("original-file-download");
        let mut m = manager(root.clone(), client).await;
        m.enqueue(vec![track.clone()]);
        finish(&mut m, Status::Complete).await;
        let path = m.library.lookup(&track.id).await.unwrap();
        assert_eq!(
            path.strip_prefix(&root).unwrap(),
            server_path(ORIGINAL).unwrap()
        );
        assert_eq!(tokio::fs::read(path).await.unwrap(), bytes);
        assert_eq!(server.count("/rest/download.view"), 1);
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn legacy_index_is_only_a_hint_and_is_never_rewritten() {
        let server = server(vec![1; 4096], false).await;
        let root = self::root("legacy-hint");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let track = song("a", 4096);
        let relative = candidates(&track)[0].clone();
        let mut job = Job::new(track.clone());
        job.status = Status::Complete;
        job.relative = Some(relative.clone());
        job.bytes = 4096;
        job.sha256 = Some("stale legacy checksum must not decide availability".into());
        let mut stale_claim = job.clone();
        stale_claim.track.id = song("b", 4096).id;
        let bytes = serde_json::to_vec(&Memory {
            version: 1,
            paused: false,
            jobs: vec![job, stale_claim],
        })
        .unwrap();
        let index = safe_path(&root, Path::new(".sonicfast/index.json"), true)
            .await
            .unwrap();
        tokio::fs::write(&index, &bytes).await.unwrap();
        let m = manager(root.clone(), server.client()).await;
        assert_eq!(lock(&m.library.index).jobs[0].status, Status::Remote);
        assert!(m.library.lookup(&track.id).await.is_none());
        let path = safe_path(&root, &relative, true).await.unwrap();
        tokio::fs::write(&path, vec![1; 4096]).await.unwrap();
        assert_eq!(m.library.lookup(&track.id).await, Some(path));
        m.shutdown().await;
        assert_eq!(tokio::fs::read(&index).await.unwrap(), bytes);
        tokio::fs::remove_file(&index).await.unwrap();
        let m = manager(root.clone(), server.client()).await;
        m.library.register(std::slice::from_ref(&track));
        assert!(m.library.lookup(&track.id).await.is_some());
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn queue_persists_pauses_deduplicates_and_prefers_local_with_stream_fallback() {
        let bytes = vec![0x46; 64 * 1024];
        let server = server(bytes.clone(), false).await;
        let root = self::root("queue");
        let mut m = manager(root.clone(), server.client()).await;
        m.control(None, "pause");
        m.enqueue(vec![
            song("a", bytes.len() as u64),
            song("a", bytes.len() as u64),
        ]);
        let pending = serde_json::from_slice(&serde_json::to_vec(&m.pending()).unwrap()).unwrap();
        assert!(!root.join(".sonicfast/index.json").exists());
        m.tick().await;
        assert_eq!(server.count("/rest/download.view"), 0);
        assert_eq!(lock(&m.library.index).jobs.len(), 1);
        assert!(
            Manager::new(root.clone(), server.client(), Arc::new(|_| {}))
                .await
                .is_err(),
            "folder lease prevents another writer"
        );
        m.shutdown().await;
        let mut m = manager(root.clone(), server.client()).await;
        m.restore(pending).unwrap();
        assert!(lock(&m.library.index).paused);
        m.control(None, "resume");
        finish(&mut m, Status::Complete).await;
        assert_eq!(server.count("/rest/download.view"), 1);
        let id = song("a", bytes.len() as u64).id;
        let path = m.library.lookup(&id).await.unwrap();
        assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes);
        let provider = LocalFirst {
            client: server.client(),
            library: Arc::new(Mutex::new(Some(m.library.clone()))),
        };
        let mut stream = provider.open(&id).await.unwrap();
        assert!(stream.is_seekable());
        stream.seek_to(123).await.unwrap();
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            bytes[123..123 + l::STREAM_CHUNK_BYTES]
        );
        assert_eq!(server.count("/rest/stream.view"), 0);
        drop(stream);
        tokio::fs::remove_file(&path).await.unwrap();
        assert!(provider.open(&id).await.is_err());
        assert_eq!(server.count("/rest/stream.view"), 1);
        assert_eq!(lock(&m.library.index).jobs[0].status, Status::Remote);
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn original_filename_adoption_and_collision_suffixes_need_no_scan() {
        let bytes = vec![1; 4096];
        let server = server(bytes.clone(), false).await;
        let root = self::root("names");
        let mut m = manager(root.clone(), server.client()).await;
        let a = song("a", 4096);
        let natural = candidates(&a)[0].clone();
        let path = safe_path(&root, &natural, true).await.unwrap();
        tokio::fs::write(&path, &bytes).await.unwrap();
        m.probe(vec![a.clone()]);
        finish(&mut m, Status::Complete).await;
        assert_eq!(server.count("/rest/download.view"), 0);
        // The same-sized name claimed by another server-scoped song is not adopted.
        m.enqueue(vec![song("b", 4096), song("c", 4096)]);
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            m.tick().await;
            if lock(&m.library.index)
                .jobs
                .iter()
                .filter(|j| j.status == Status::Complete)
                .count()
                == 3
            {
                break;
            }
            assert!(Instant::now() < until);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(server.count("/rest/download.view"), 2);
        let jobs = lock(&m.library.index).jobs.clone();
        assert_ne!(jobs[1].relative, jobs[2].relative);
        assert_eq!(tokio::fs::read(path).await.unwrap(), bytes);
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[test]
    fn server_layout_is_preferred_and_untrusted_paths_cannot_escape() {
        let mut track = song("a", 4096);
        track.file_path = Some(
            "Bad Astronaut/Houston: We Have a Drinking Problem/Disc 1/Solar Sister.m4a".into(),
        );
        let expected = if cfg!(windows) {
            "Bad Astronaut/Houston_ We Have a Drinking Problem/Disc 1/Solar Sister.m4a"
        } else {
            "Bad Astronaut/Houston: We Have a Drinking Problem/Disc 1/Solar Sister.m4a"
        };
        assert_eq!(candidates(&track)[0], PathBuf::from(expected));
        for path in [
            "/absolute/file.flac",
            "../escape.flac",
            "Artist/../file.flac",
            "C:\\Music\\file.flac",
            "\\\\server\\share\\file.flac",
            ".sonicfast/index.json",
            "Artist//file.flac",
        ] {
            assert!(server_path(path).is_none(), "must reject {path}");
        }
    }
    #[tokio::test]
    async fn new_downloads_use_natural_names_and_legacy_hashed_files_stay_discoverable() {
        let server = server(vec![1; 4096], false).await;
        let root = self::root("natural");
        let mut m = manager(root.clone(), server.client()).await;
        let track = song("a", 4096);
        m.enqueue(vec![track.clone()]);
        finish(&mut m, Status::Complete).await;
        assert_eq!(
            lock(&m.library.index).jobs[0].relative.as_deref(),
            Some(Path::new("Artist/Album/original.flac"))
        );
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();

        let root = self::root("legacy-names");
        let m = manager(root.clone(), server.client()).await;
        let mut track = track;
        track.album.as_mut().unwrap().title = "Album: Name".into();
        track.file_path = Some("Artist/Album: Name/original.flac".into());
        let legacy = metadata_candidates(&track, portable_component)
            .pop()
            .unwrap();
        let path = safe_path(&root, &legacy, true).await.unwrap();
        tokio::fs::write(&path, vec![1; 4096]).await.unwrap();
        m.library.register(std::slice::from_ref(&track));
        assert_eq!(m.library.lookup(&track.id).await, Some(path));
        assert_eq!(
            server.count("/rest/download.view"),
            1,
            "legacy adoption must not redownload"
        );
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn concurrent_transfers_reserve_natural_names_without_overwriting_unrelated_files() {
        let server = server(vec![1; 4096], false).await;
        let root = self::root("reserve");
        let mut m = manager(root.clone(), server.client()).await;
        m.enqueue(vec![song("a", 4096), song("b", 4096)]);
        let a = song("a", 4096);
        let b = song("b", 4096);
        let (first, second) =
            tokio::join!(destination(&m.library, &a), destination(&m.library, &b));
        let (first, first_path) = first.unwrap();
        let (second, _) = second.unwrap();
        let natural = PathBuf::from("Artist/Album/original.flac");
        assert!(first == natural || second == natural);
        assert_ne!(first, second);
        tokio::fs::write(&first_path, b"keep this file")
            .await
            .unwrap();
        let (replacement, replacement_path) = destination(&m.library, &a).await.unwrap();
        assert_ne!(replacement, first);
        assert_eq!(
            tokio::fs::read(first_path).await.unwrap(),
            b"keep this file"
        );
        tokio::fs::write(&replacement_path, vec![1; 4096])
            .await
            .unwrap();
        m.shutdown().await;
        let restored = manager(root.clone(), server.client()).await;
        restored.library.register(std::slice::from_ref(&a));
        assert_eq!(restored.library.lookup(&a.id).await, Some(replacement_path));
        assert!(!root.join(".sonicfast/index.json").exists());
        restored.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    #[ignore = "requires explicit live URL, API key and track ID; downloads one original into an isolated cache"]
    async fn live_original_download_preserves_server_relative_path() {
        let client = Arc::new(
            Client::new(
                l::ServerId::new("live-download-path-check").unwrap(),
                &std::env::var("LIBRESONIC_TEST_URL").expect("explicit live URL required"),
                l::opensubsonic::Credentials::api_key(
                    std::env::var("LIBRESONIC_TEST_API_KEY").expect("explicit API key required"),
                ),
                l::opensubsonic::ClientOptions::default().allow_http(true),
            )
            .unwrap(),
        );
        let id = MediaId::new(
            l::ServerId::new("live-download-path-check").unwrap(),
            l::MediaKind::Track,
            std::env::var("LIBRESONIC_TEST_TRACK_ID").expect("explicit track ID required"),
        )
        .unwrap();
        let track = client.song(&id).await.unwrap();
        let expected = server_path(
            track
                .file_path
                .as_deref()
                .expect("server must expose a relative path"),
        )
        .unwrap();
        let root = self::root("live-path");
        let mut m = manager(root.clone(), client.clone()).await;
        m.enqueue(vec![track.clone()]);
        finish(&mut m, Status::Complete).await;
        let job = lock(&m.library.index).jobs[0].clone();
        assert_eq!(job.relative, Some(expected.clone()));
        assert_eq!(Some(job.bytes), track.file_size);
        assert!(job.sha256.is_some());
        m.shutdown().await;
        let restored = manager(root.clone(), client).await;
        restored.library.register(std::slice::from_ref(&track));
        assert_eq!(
            restored.library.lookup(&id).await,
            Some(root.join(&expected))
        );
        assert!(!root.join(".sonicfast/index.json").exists());
        restored.shutdown().await;
        eprintln!(
            "Live original downloaded and restored at {}",
            expected.display()
        );
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn cancellation_removes_partial_and_resume_does_not_restart_cancelled_items() {
        let server = server(vec![3; 1024 * 1024], true).await;
        let root = self::root("cancel");
        let mut m = manager(root.clone(), server.client()).await;
        let track = song("a", 1024 * 1024);
        let id = track.id.clone();
        m.enqueue(vec![track]);
        m.tick().await;
        let until = Instant::now() + Duration::from_secs(3);
        while server.count("/rest/download.view") == 0 {
            assert!(Instant::now() < until);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        m.control(Some(&id), "cancel");
        assert_eq!(
            m.pending().jobs[0].status,
            Status::Cancelled,
            "closing immediately after cancel must not restore this transfer as queued"
        );
        finish(&mut m, Status::Cancelled).await;
        m.control(None, "resume");
        m.tick().await;
        assert_eq!(lock(&m.library.index).jobs[0].status, Status::Cancelled);
        let mut entries = tokio::fs::read_dir(root.join(".sonicfast/partials"))
            .await
            .unwrap();
        assert!(entries.next_entry().await.unwrap().is_none());
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn corrupted_index_is_preserved_and_untrusted_components_stay_inside_root() {
        let root = self::root("invalid");
        let server = server(vec![1; 32], false).await;
        tokio::fs::create_dir_all(root.join(".sonicfast"))
            .await
            .unwrap();
        let index = root.join(".sonicfast/index.json");
        tokio::fs::write(&index, b"not json").await.unwrap();
        let m = Manager::new(root.clone(), server.client(), Arc::new(|_| {}))
            .await
            .unwrap();
        m.shutdown().await;
        assert_eq!(tokio::fs::read(&index).await.unwrap(), b"not json");
        let mut track = song("a", 32);
        track.title = "../../outside".into();
        track.artists[0].name = "../CON".into();
        track.album.as_mut().unwrap().title = "../Album".into();
        track.file_path = Some("../../NUL.flac".into());
        for path in candidates(&track) {
            assert_eq!(path.components().count(), 3);
            assert!(path.components().all(|p| matches!(p, Component::Normal(_))));
        }
        assert!(
            safe_path(&root, Path::new("../outside"), true)
                .await
                .is_err()
        );
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn transient_downloads_retry_with_backoff_and_size_mismatches_never_publish() {
        use std::sync::atomic::AtomicUsize;
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let server=Server::start(move |request| {
            if request.path=="/rest/getSong.view" {Reply::json(serde_json::json!({"status":"ok","song":{"id":"a","title":"Retry","artist":"Artist","album":"Album","suffix":"flac","size":4096}}))}
            else if seen.fetch_add(1,Ordering::SeqCst)<2 {let mut r=Reply::audio(vec![]);r.status=503;r}
            else {Reply::audio(vec![1;4096])}
        }).await;
        let root = self::root("retry");
        let mut m = manager(root.clone(), server.client()).await;
        m.enqueue(vec![song("a", 4096)]);
        for attempt in 1..=2 {
            finish(&mut m, Status::Retrying).await;
            {
                let mut index = lock(&m.library.index);
                let job = &mut index.jobs[0];
                assert_eq!(job.attempts, attempt);
                assert!(job.retry_at > now());
                job.retry_at = 0;
            }
            m.tick().await;
        }
        finish(&mut m, Status::Complete).await;
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
        let server=Server::start(|request|if request.path=="/rest/getSong.view" {Reply::json(serde_json::json!({"status":"ok","song":{"id":"a","title":"Mismatch","artist":"Artist","album":"Album","suffix":"flac","size":4096}}))} else {Reply::audio(vec![1;40])}).await;
        let root = self::root("size");
        let mut m = manager(root.clone(), server.client()).await;
        m.enqueue(vec![song("a", 4096)]);
        finish(&mut m, Status::Failed).await;
        assert_eq!(lock(&m.library.index).jobs[0].attempts, 1);
        assert!(lock(&m.library.index).jobs[0].sha256.is_none());
        assert!(
            tokio::fs::read_dir(root.join(".sonicfast/partials"))
                .await
                .unwrap()
                .next_entry()
                .await
                .unwrap()
                .is_none()
        );
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn modified_local_content_is_rejected_even_when_its_size_stays_the_same() {
        let server = server(vec![1; 4096], false).await;
        let root = self::root("changed");
        let mut m = manager(root.clone(), server.client()).await;
        let track = song("a", 4096);
        m.enqueue(vec![track.clone()]);
        finish(&mut m, Status::Complete).await;
        let path = m.library.lookup(&track.id).await.unwrap();
        tokio::fs::write(&path, vec![2; 4096]).await.unwrap();
        std::fs::File::open(&path)
            .unwrap()
            .set_modified(SystemTime::now() + Duration::from_secs(1))
            .unwrap();
        assert!(m.library.lookup(&track.id).await.is_none());
        assert_eq!(lock(&m.library.index).jobs[0].status, Status::Remote);
        m.enqueue(vec![track.clone()]);
        finish(&mut m, Status::Complete).await;
        let replacement = m.library.lookup(&track.id).await.unwrap();
        assert_ne!(replacement, path, "changed files must never be overwritten");
        assert_eq!(tokio::fs::read(&path).await.unwrap(), vec![2; 4096]);
        assert_eq!(tokio::fs::read(replacement).await.unwrap(), vec![1; 4096]);
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_music_directories_are_rejected() {
        let server = server(vec![1; 4096], false).await;
        let root = self::root("symlink");
        let outside = root.with_extension("outside");
        tokio::fs::create_dir_all(&outside).await.unwrap();
        let mut m = manager(root.clone(), server.client()).await;
        std::os::unix::fs::symlink(&outside, root.join("Artist")).unwrap();
        m.enqueue(vec![song("a", 4096)]);
        finish(&mut m, Status::Failed).await;
        assert!(
            tokio::fs::read_dir(&outside)
                .await
                .unwrap()
                .next_entry()
                .await
                .unwrap()
                .is_none()
        );
        m.shutdown().await;
        tokio::fs::remove_dir_all(root).await.unwrap();
        tokio::fs::remove_dir_all(outside).await.unwrap();
    }
}
