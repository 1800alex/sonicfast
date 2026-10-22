//! Serialized, checkpointed at-most-once reporting; no ambiguous-write retries.
use libresonic::{MusicService, opensubsonic::Client, playback::scrobble::Scrobble};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc;
type Gate = Arc<tokio::sync::Mutex<()>>;
type Gates = std::collections::HashMap<PathBuf, std::sync::Weak<tokio::sync::Mutex<()>>>;
fn gate(path: &std::path::Path) -> Gate {
    static GATES: std::sync::OnceLock<std::sync::Mutex<Gates>> = std::sync::OnceLock::new();
    let mut gates = GATES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gates.retain(|_, g| g.strong_count() > 0);
    if let Some(old) = gates.get(path).and_then(std::sync::Weak::upgrade) {
        return old;
    }
    let value = Arc::new(tokio::sync::Mutex::new(()));
    gates.insert(path.to_path_buf(), Arc::downgrade(&value));
    value
}
pub struct Reporter {
    tx: mpsc::Sender<Scrobble>,
    task: tokio::task::JoinHandle<()>,
}
impl Reporter {
    pub fn spawn(
        client: Arc<Client>,
        path: PathBuf,
        notice: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<Scrobble>(64);
        let gate = gate(&path);
        let task = tokio::spawn(async move {
            while let Some(report) = rx.recv().await {
                let guard = gate.clone().lock_owned().await;
                let mut receipts = match tokio::fs::metadata(&path).await {
                    Ok(meta) if meta.len() <= 1024 * 1024 => match tokio::fs::read(&path)
                        .await
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
                    {
                        Some(receipts) => receipts,
                        None => {
                            notice("Unable to read scrobble receipts; reporting is stopped to prevent duplicates".into());
                            return;
                        }
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                    _ => {
                        notice("Unable to read scrobble receipts; reporting is stopped to prevent duplicates".into());
                        return;
                    }
                };
                let key = format!("{}:{}", report.occurrence, report.submission);
                if receipts.contains(&key) {
                    continue;
                }
                receipts.push(key);
                if receipts.len() > 4096 {
                    receipts.drain(..receipts.len() - 4096);
                }
                let saved = receipts.clone();
                let file = path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    // Retain serialization even if this reporter is cancelled mid-write.
                    let _guard = guard;
                    let bytes = serde_json::to_vec(&saved).map_err(|_| ())?;
                    fastframe_file::write_private(&file, bytes).map_err(|_| ())
                })
                .await;
                if !matches!(result, Ok(Ok(()))) {
                    notice("Unable to save scrobble receipts; no report was sent".into());
                    continue;
                }
                if let Err(error) = client
                    .scrobble(&report.track, report.started_ms, report.submission)
                    .await
                {
                    notice(format!(
                        "Couldn't report listening: {error}. This report will not be retried automatically"
                    ));
                }
            }
        });
        Self { tx, task }
    }
    pub fn send(&self, report: Scrobble) -> bool {
        self.tx.try_send(report).is_ok()
    }
}
impl Drop for Reporter {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsonic::test_server::{Reply, Server};
    #[tokio::test]
    async fn receipts_prevent_replay_across_restart_and_ambiguous_failures_are_not_retried() {
        let server = Server::start(|_| {
            let mut r = Reply::json(serde_json::json!({"status":"ok"}));
            r.status = 500;
            r
        })
        .await;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/scrobble-tests")
            .join(format!("{:016x}.json", rand::random::<u64>()));
        let report = Scrobble {
            track: libresonic::MediaId::new(
                libresonic::ServerId::new("fixture").unwrap(),
                libresonic::MediaKind::Track,
                "a",
            )
            .unwrap(),
            occurrence: 123,
            started_ms: 1000,
            submission: true,
        };
        let reporter = Reporter::spawn(server.client(), path.clone(), Arc::new(|_| {}));
        assert!(reporter.send(report.clone()));
        assert!(reporter.send(report.clone()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while server.count("/rest/scrobble.view") == 0 {
            assert!(std::time::Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert_eq!(server.count("/rest/scrobble.view"), 1);
        drop(reporter);
        let reporter = Reporter::spawn(server.client(), path.clone(), Arc::new(|_| {}));
        assert!(reporter.send(report.clone()));
        let next = Scrobble {
            occurrence: 124,
            ..report
        };
        assert!(reporter.send(next));
        while server.count("/rest/scrobble.view") < 2 {
            assert!(std::time::Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(server.requests.lock().unwrap()[0].params["time"], "1000");
        drop(reporter);
        let _ = tokio::fs::remove_file(path).await;
    }
}
