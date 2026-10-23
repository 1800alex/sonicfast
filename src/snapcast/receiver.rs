use super::{Settings, Shared};
use libresonic::playback::Cancellation;
use std::time::Duration;
use tokio::{sync::watch, task::JoinHandle};

/// Owns a built-in receiver task and the resolved sender endpoint.
pub struct Manager {
    pub shared: Shared,
    config: watch::Sender<Option<Settings>>,
    task: JoinHandle<()>,
}
impl Manager {
    pub fn new() -> Self {
        let shared = Shared::default();
        let (config, changes) = watch::channel(None);
        let state = shared.clone();
        let task = tokio::spawn(supervise(state, changes));
        Self {
            shared,
            config,
            task,
        }
    }
    pub fn configure(&self, mut settings: Settings) -> Result<(), String> {
        settings.validate()?;
        if settings.receive && settings.client_id.is_empty() {
            settings.client_id = format!("sonicfast-{:016x}", rand::random::<u64>());
        }
        {
            let mut state = self
                .shared
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.settings = settings.clone();
            state.targets.clear();
            state.status.sender = if settings.send {
                "Resolving source address"
            } else {
                "Disabled"
            }
            .into();
            state.status.receiver = if settings.receive {
                "Starting receiver"
            } else {
                "Disabled"
            }
            .into();
        }
        self.config.send_replace(Some(settings));
        Ok(())
    }
    pub async fn shutdown(self) {
        drop(self.config);
        let _ = self.task.await;
    }
}
impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}
async fn supervise(shared: Shared, mut changes: watch::Receiver<Option<Settings>>) {
    let mut receiver: Option<(Cancellation, JoinHandle<Result<(), String>>)> = None;
    loop {
        if changes.changed().await.is_err() {
            break;
        }
        let current = changes.borrow_and_update().clone().unwrap_or_default();
        if let Some((cancel, task)) = receiver.take() {
            cancel.cancel();
            let _ = task.await;
        }
        if current.send {
            let lookup = tokio::net::lookup_host((current.host.as_str(), current.source_port));
            let result = tokio::select! {
                result=tokio::time::timeout(Duration::from_secs(3),lookup)=>Some(result),
                change=changes.changed()=>{if change.is_err(){break;}changes.mark_changed();None},
            };
            if let Some(result) = result {
                let targets: Vec<_> = result
                    .ok()
                    .and_then(Result::ok)
                    .map(|addresses| addresses.take(8).collect())
                    .unwrap_or_default();
                let mut state = shared
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.targets = targets;
                state.status.sender = if !state.targets.is_empty() {
                    "Ready; waiting for playback"
                } else {
                    "Source address unavailable; check the hostname or reconnect"
                }
                .into();
            } else {
                continue;
            }
        }
        if current.receive {
            let cancel = Cancellation::default();
            let token = cancel.clone();
            let state = shared.clone();
            receiver = Some((
                cancel,
                tokio::spawn(super::native::receive(current, state, token, None)),
            ));
        } else {
            shared.receiver_status("Disabled");
        }
    }
    if let Some((cancel, task)) = receiver.take() {
        cancel.cancel();
        let _ = task.await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unavailable_server_is_reported_and_shutdown_is_bounded() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let manager = Manager::new();
        manager
            .configure(Settings {
                receive: true,
                allow_insecure: true,
                stream_port: port,
                ..Default::default()
            })
            .unwrap();
        tokio::time::sleep(Duration::from_millis(350)).await;
        assert!(manager.shared.status().receiver.contains("Cannot connect"));
        tokio::time::timeout(Duration::from_secs(4), manager.shutdown())
            .await
            .unwrap();
    }
}
