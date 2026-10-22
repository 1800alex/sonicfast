//! Non-secret profiles and a separate native credential namespace.
use crate::credentials::{NativeStore, ProtectedStore};
use libresonic::{
    ServerId,
    opensubsonic::{Client, ClientOptions, Credentials},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub api_key: bool,
    pub allow_http: bool,
    pub remember: bool,
}
impl Profile {
    pub fn client(&self, secret: &str) -> Result<Client, libresonic::Error> {
        Client::new(
            ServerId::new(&self.id)?,
            &self.url,
            if self.api_key {
                Credentials::api_key(secret)
            } else {
                Credentials::token(&self.username, secret)
            },
            ClientOptions::default().allow_http(self.allow_http),
        )
    }
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Profiles {
    pub selected: Option<String>,
    pub profiles: Vec<Profile>,
}
impl Profiles {
    pub fn load(path: &std::path::Path) -> Self {
        use std::io::Read;
        let mut bytes = Vec::new();
        let Ok(file) = std::fs::File::open(path) else {
            return Self::default();
        };
        if file.take(1024 * 1024 + 1).read_to_end(&mut bytes).is_err() || bytes.len() > 1024 * 1024
        {
            return Self::default();
        }
        serde_json::from_slice(&bytes).unwrap_or_default()
    }
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        if self.profiles.iter().any(|profile| {
            reqwest::Url::parse(&profile.url).ok().is_none_or(|url| {
                !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
            })
        }) {
            return Err(
                "Server profile URL must not contain credentials, queries or fragments".into(),
            );
        }
        let bytes = serde_json::to_vec(self).map_err(|_| "Unable to encode server profiles")?;
        fastframe_file::write_private(path, bytes)
            .map_err(|_| "Unable to save server profiles".into())
    }
}
// No Debug or Serialize on the form: it contains a usable secret.
#[derive(Default)]
pub struct Form {
    pub profile: Profile,
    pub secret: String,
    pub profiles: Vec<Profile>,
    pub notice: Option<String>,
}

// Native calls stay on one dedicated bounded worker, even if the desktop store locks.
type Job = Box<dyn FnOnce(&mut NativeStore) + Send>;
#[derive(Clone)]
pub struct Secrets {
    jobs: std::sync::mpsc::SyncSender<Job>,
    pub generation: Arc<AtomicU64>,
    scope: String,
}
impl Secrets {
    pub fn new(state: PathBuf) -> Self {
        use sha2::{Digest, Sha256};
        let scope = format!("{:x}", Sha256::digest(state.to_string_lossy().as_bytes()));
        let (jobs, receiver) = std::sync::mpsc::sync_channel::<Job>(16);
        let runtime = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let _entered = runtime.enter();
            let mut store = NativeStore::sonicfast();
            while let Ok(job) = receiver.recv() {
                job(&mut store);
            }
        });
        Self {
            jobs,
            generation: Arc::new(AtomicU64::new(0)),
            scope,
        }
    }
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
    pub async fn access(
        &self,
        id: &str,
        write: Option<Option<String>>,
    ) -> Result<Option<String>, String> {
        let key = format!("{}:{id}", self.scope);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let generation = self.generation.load(Ordering::SeqCst);
        let current = self.generation.clone();
        self.jobs
            .try_send(Box::new(move |store| {
                let result = (|| {
                    if current.load(Ordering::SeqCst) != generation {
                        return Err(crate::credentials::Error::Stale);
                    }
                    match write {
                        Some(Some(secret)) => {
                            store.write(&key, secret.as_bytes())?;
                            if store.read(&key)?.as_deref() != Some(secret.as_bytes()) {
                                return Err(crate::credentials::Error::Verification);
                            }
                            if current.load(Ordering::SeqCst) != generation {
                                let _ = store.delete(&key);
                                return Err(crate::credentials::Error::Stale);
                            }
                            Ok(None)
                        }
                        Some(None) => {
                            store.delete(&key)?;
                            Ok(None)
                        }
                        None => store
                            .read(&key)?
                            .map(|b| {
                                String::from_utf8(b).map_err(|_| crate::credentials::Error::Invalid)
                            })
                            .transpose(),
                    }
                })()
                .map_err(|_| {
                    "Unlock or enable the system credential store to remember this server sign-in."
                        .to_string()
                });
                let _ = tx.send(result);
            }))
            .map_err(|_| "Credential worker is busy".to_string())?;
        tokio::time::timeout(Duration::from_secs(20), rx)
            .await
            .map_err(|_| "Credential store did not respond".to_string())?
            .map_err(|_| "Credential worker stopped".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires an unlocked native system credential store; use the isolated Phase 3 runner"]
    async fn native_server_secret_round_trip_and_revocation() {
        let state =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/phase-3-native/test-profile");
        let secrets = Secrets::new(state);
        let id = format!("fixture-{}", rand::random::<u64>());
        secrets
            .access(&id, Some(Some("fixture-secret-only".into())))
            .await
            .unwrap();
        assert_eq!(
            secrets.access(&id, None).await.unwrap().as_deref(),
            Some("fixture-secret-only")
        );
        secrets.invalidate();
        secrets.access(&id, Some(None)).await.unwrap();
        assert_eq!(secrets.access(&id, None).await.unwrap(), None);
    }
    #[test]
    fn profiles_contain_no_authentication_material_and_reject_credential_urls() {
        let p = Profile {
            id: "one".into(),
            name: "Fixture".into(),
            username: "alice".into(),
            url: "https://alice:secret@host".into(),
            ..Default::default()
        };
        let error = p.client("secret").unwrap_err();
        assert!(!format!("{error:?}").contains("secret"));
        let encoded = serde_json::to_string(&p).unwrap();
        assert!(!encoded.contains("password"));
    }
}
