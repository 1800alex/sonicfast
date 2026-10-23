//! Native on-screen PIN pairing and authenticated receiver identity verification.
use super::{crypto::*, wire::Wire};
use libresonic::playback::Cancellation;
use ring::signature::KeyPair;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum Error {
    #[error("AirPlay receiver did not respond; check its power and network, then retry")]
    Network,
    #[error("AirPlay pairing or receiver verification failed; check the PIN or pair again")]
    Authentication,
    #[error("The AirPlay receiver is busy or has limited pairing attempts; wait before retrying")]
    Backoff,
    #[error("The receiver returned an unsupported AirPlay pairing response")]
    Protocol,
    #[error("AirPlay pairing was cancelled or expired")]
    Cancelled,
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Network
    }
}

/// Deliberately has no Debug implementation. Saved only in the protected store.
#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    seed: [u8; 32],
    receiver_key: [u8; 32],
    receiver_id: Vec<u8>,
    client_id: Vec<u8>,
}
impl Credentials {
    pub(crate) fn valid(&self) -> bool {
        !self.receiver_id.is_empty()
            && self.receiver_id.len() <= 256
            && !self.client_id.is_empty()
            && self.client_id.len() <= 256
    }
}
pub struct Pin(pub String);
impl std::fmt::Debug for Pin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pin([redacted])")
    }
}
impl Clone for Pin {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub struct Setup {
    wire: Wire,
    salt: Vec<u8>,
    server: Vec<u8>,
    until: Instant,
    cancel: Cancellation,
}
impl Setup {
    pub fn start(address: SocketAddr, cancel: Cancellation) -> Result<Self> {
        let mut wire = Wire::connect(address)?;
        wire.post("/pair-pin-start", &[], 1, &cancel)?;
        let reply = wire.post("/pair-setup", &encode(&[(0, &[0]), (6, &[1])]), 2, &cancel)?;
        let fields = step(&reply, 2)?;
        Ok(Self {
            wire,
            salt: field(&fields, 2, 16..=16)?.to_vec(),
            server: field(&fields, 3, 1..=384)?.to_vec(),
            until: Instant::now() + Duration::from_secs(120),
            cancel,
        })
    }
    pub fn finish(mut self, pin: Pin) -> Result<Credentials> {
        if self.cancel.is_cancelled() || Instant::now() >= self.until {
            return Err(Error::Cancelled);
        }
        if pin.0.len() != 4 || !pin.0.bytes().all(|v| v.is_ascii_digit()) {
            return Err(Error::Authentication);
        }
        let private: [u8; 32] = rand::random();
        let srp = Srp::new(&pin.0, &self.salt, &self.server, &private)?;
        let reply = self.wire.post(
            "/pair-setup",
            &encode(&[(6, &[3]), (3, &srp.public), (4, &srp.proof)]),
            3,
            &self.cancel,
        )?;
        let fields = step(&reply, 4)?;
        srp.verify(field(&fields, 4, 64..=64)?)?;
        let seed: [u8; 32] = rand::random();
        let signing = signing(&seed)?;
        let client_id = format!("{:032x}", rand::random::<u128>()).into_bytes();
        let controller_x = derive(
            &srp.key,
            "Pair-Setup-Controller-Sign-Salt",
            "Pair-Setup-Controller-Sign-Info",
        )?;
        let sig = signing.sign(
            &[
                controller_x.as_slice(),
                &client_id,
                signing.public_key().as_ref(),
            ]
            .concat(),
        );
        let mut encrypted = encode(&[
            (1, &client_id),
            (3, signing.public_key().as_ref()),
            (10, sig.as_ref()),
        ]);
        let key = derive(
            &srp.key,
            "Pair-Setup-Encrypt-Salt",
            "Pair-Setup-Encrypt-Info",
        )?;
        seal(&key, b"PS-Msg05", &[], &mut encrypted)?;
        let reply = self.wire.post(
            "/pair-setup",
            &encode(&[(6, &[5]), (5, &encrypted)]),
            4,
            &self.cancel,
        )?;
        let fields = step(&reply, 6)?;
        let decrypted = open(&key, b"PS-Msg06", &[], field(&fields, 5, 16..=4096)?)?;
        let fields = decode(&decrypted)?;
        let receiver_id = field(&fields, 1, 1..=256)?.to_vec();
        let receiver_key: [u8; 32] = field(&fields, 3, 32..=32)?.try_into().unwrap();
        let accessory_x = derive(
            &srp.key,
            "Pair-Setup-Accessory-Sign-Salt",
            "Pair-Setup-Accessory-Sign-Info",
        )?;
        verify(
            &receiver_key,
            &[accessory_x.as_slice(), &receiver_id, &receiver_key].concat(),
            field(&fields, 10, 64..=64)?,
        )?;
        Ok(Credentials {
            seed,
            receiver_key,
            receiver_id,
            client_id,
        })
    }
}
pub(super) fn verify_pairing(
    wire: &mut Wire,
    credentials: &Credentials,
    cancel: &Cancellation,
) -> Result<Vec<u8>> {
    if !credentials.valid() {
        return Err(Error::Authentication);
    }
    let (private, public) = exchange()?;
    let reply = wire.post(
        "/pair-verify",
        &encode(&[(6, &[1]), (3, &public)]),
        1,
        cancel,
    )?;
    let fields = step(&reply, 2)?;
    let receiver_public = field(&fields, 3, 32..=32)?;
    let shared = agree(private, receiver_public)?;
    let key = derive(
        &shared,
        "Pair-Verify-Encrypt-Salt",
        "Pair-Verify-Encrypt-Info",
    )?;
    let bytes = open(&key, b"PV-Msg02", &[], field(&fields, 5, 16..=4096)?)?;
    let identity = decode(&bytes)?;
    let id = field(&identity, 1, 1..=256)?;
    if id != credentials.receiver_id {
        return Err(Error::Authentication);
    }
    verify(
        &credentials.receiver_key,
        &[receiver_public, id, &public].concat(),
        field(&identity, 10, 64..=64)?,
    )?;
    let signing = signing(&credentials.seed)?;
    let sig = signing.sign(&[public.as_slice(), &credentials.client_id, receiver_public].concat());
    let mut bytes = encode(&[(1, &credentials.client_id), (10, sig.as_ref())]);
    seal(&key, b"PV-Msg03", &[], &mut bytes)?;
    let reply = wire.post(
        "/pair-verify",
        &encode(&[(6, &[3]), (5, &bytes)]),
        2,
        cancel,
    )?;
    step(&reply, 4)?;
    wire.encrypt(
        derive(&shared, "Control-Salt", "Control-Write-Encryption-Key")?,
        derive(&shared, "Control-Salt", "Control-Read-Encryption-Key")?,
    );
    Ok(shared)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Idle,
    Starting,
    Pin,
    Finishing,
    Paired,
    Failed,
}
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub attempt: u64,
    pub settings: crate::casting::Settings,
    pub stage: Stage,
    pub message: String,
}
#[derive(Default)]
pub(crate) struct State {
    status: Status,
    pending: Option<Setup>,
    cancel: Cancellation,
    credentials: std::collections::HashMap<String, std::sync::Arc<Credentials>>,
    restoring: std::collections::HashSet<String>,
    store: Option<crate::subsonic::profile::Secrets>,
}
fn endpoint(settings: &crate::casting::Settings) -> String {
    format!(
        "airplay:{}:{}",
        settings.host.to_ascii_lowercase(),
        settings.port
    )
}
impl crate::casting::Shared {
    pub fn with_pairing_store(path: std::path::PathBuf) -> Self {
        let shared = Self::default();
        shared.lock().pairing.store = Some(crate::subsonic::profile::Secrets::new(path));
        shared
    }
    pub fn pairing_status(&self) -> Status {
        let mut state = self.lock();
        let pairing = &mut state.pairing;
        if pairing
            .pending
            .as_ref()
            .is_some_and(|v| Instant::now() >= v.until)
        {
            pairing.pending.take();
            pairing.cancel.cancel();
            pairing.status.stage = Stage::Failed;
            pairing.status.message = Error::Cancelled.to_string();
        }
        pairing.status.clone()
    }
    pub fn cancel_pairing(&self, attempt: u64) {
        let mut state = self.lock();
        let p = &mut state.pairing;
        if p.status.attempt != attempt {
            return;
        }
        p.cancel.cancel();
        p.pending.take();
        if let Some(store) = &p.store {
            store.invalidate();
        }
        p.status.stage = Stage::Idle;
    }
    pub fn begin_pairing(&self, mut settings: crate::casting::Settings, force: bool) {
        settings.output = Some(crate::casting::Kind::AirPlay);
        settings.airplay2 = true;
        if settings.validate().is_err() {
            return;
        }
        let (attempt, cancel) = {
            let mut state = self.lock();
            let p = &mut state.pairing;
            p.cancel.cancel();
            p.pending.take();
            if let Some(store) = &p.store {
                store.invalidate();
            }
            let attempt = p.status.attempt.wrapping_add(1);
            p.cancel = Cancellation::default();
            p.status = Status {
                attempt,
                settings: settings.clone(),
                stage: Stage::Starting,
                message: "Requesting an AirPlay code from the receiver…".into(),
            };
            (attempt, p.cancel.clone())
        };
        let shared = self.clone();
        tokio::spawn(async move {
            if !force {
                let key = endpoint(&settings);
                let (known, store) = {
                    let state = shared.lock();
                    (
                        state.pairing.credentials.get(&key).cloned(),
                        state.pairing.store.clone(),
                    )
                };
                let credentials = if known.is_some() {
                    known
                } else if let Some(store) = store {
                    store
                        .access(&key, None)
                        .await
                        .ok()
                        .flatten()
                        .and_then(|v| serde_json::from_str::<Credentials>(&v).ok())
                        .filter(Credentials::valid)
                        .map(std::sync::Arc::new)
                } else {
                    None
                };
                if cancel.is_cancelled() {
                    return;
                }
                if let Some(credentials) = credentials {
                    let mut state = shared.lock();
                    let p = &mut state.pairing;
                    if p.status.attempt != attempt {
                        return;
                    }
                    p.credentials.insert(key, credentials);
                    p.status.stage = Stage::Paired;
                    p.status.message = "Using remembered AirPlay pairing".into();
                    return;
                }
            }
            let addresses = tokio::time::timeout(
                Duration::from_secs(3),
                tokio::net::lookup_host((settings.host.as_str(), settings.port)),
            )
            .await;
            let address = addresses
                .ok()
                .and_then(std::result::Result::ok)
                .and_then(|mut v| v.next());
            let result = if let Some(address) = address {
                tokio::task::spawn_blocking(move || Setup::start(address, cancel))
                    .await
                    .unwrap_or(Err(Error::Network))
            } else {
                Err(Error::Network)
            };
            let mut state = shared.lock();
            let p = &mut state.pairing;
            if p.status.attempt != attempt || p.cancel.is_cancelled() {
                return;
            }
            match result {
                Ok(setup) => {
                    p.pending = Some(setup);
                    p.status.stage = Stage::Pin;
                    p.status.message = "Enter the four-digit AirPlay code shown on your TV.".into();
                }
                Err(error) => {
                    p.status.stage = Stage::Failed;
                    p.status.message = error.to_string();
                }
            }
        });
    }
    pub fn finish_pairing(&self, attempt: u64, pin: Pin) {
        let pending = {
            let mut state = self.lock();
            let p = &mut state.pairing;
            if p.status.attempt != attempt || p.status.stage != Stage::Pin {
                return;
            }
            let pending = p.pending.take();
            p.status.stage = Stage::Finishing;
            p.status.message = "Verifying AirPlay pairing…".into();
            pending
        };
        let Some(pending) = pending else {
            return;
        };
        let shared = self.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || pending.finish(pin))
                .await
                .unwrap_or(Err(Error::Network));
            let (store, key, credentials) = {
                let mut state = shared.lock();
                let p = &mut state.pairing;
                if p.status.attempt != attempt || p.cancel.is_cancelled() {
                    return;
                }
                match result {
                    Ok(credentials) => {
                        let key = endpoint(&p.status.settings);
                        let credentials = std::sync::Arc::new(credentials);
                        (p.store.clone(), key, credentials)
                    }
                    Err(error) => {
                        p.status.stage = Stage::Failed;
                        p.status.message = error.to_string();
                        return;
                    }
                }
            };
            let remembered = if let Some(store) = store {
                match serde_json::to_string(credentials.as_ref()) {
                    Ok(encoded) => store.access(&key, Some(Some(encoded))).await.is_ok(),
                    Err(_) => false,
                }
            } else {
                false
            };
            let mut state = shared.lock();
            let p = &mut state.pairing;
            if p.status.attempt != attempt || p.cancel.is_cancelled() {
                return;
            }
            p.credentials.insert(key, credentials);
            p.status.stage = Stage::Paired;
            p.status.message = if remembered { "AirPlay paired and remembered" } else { "AirPlay paired for this session. Unlock the system credential store and pair again to remember this receiver." }.into();
        });
    }
    pub(crate) fn restore_pairing(&self) {
        let job = {
            let mut state = self.lock();
            if state.settings.output != Some(crate::casting::Kind::AirPlay)
                || !state.settings.airplay2
            {
                return;
            }
            let key = endpoint(&state.settings);
            let p = &mut state.pairing;
            if p.credentials.contains_key(&key) || p.restoring.contains(&key) {
                return;
            }
            p.store.clone().map(|store| {
                p.restoring.insert(key.clone());
                (store, key)
            })
        };
        let Some((store, key)) = job else {
            return;
        };
        let shared = self.clone();
        tokio::spawn(async move {
            let result = store.access(&key, None).await;
            let mut state = shared.lock();
            let p = &mut state.pairing;
            p.restoring.remove(&key);
            if let Ok(Some(encoded)) = result
                && let Ok(credentials) = serde_json::from_str::<Credentials>(&encoded)
                && credentials.valid()
            {
                p.credentials
                    .entry(key)
                    .or_insert_with(|| std::sync::Arc::new(credentials));
            }
        });
    }
    pub(super) fn airplay_credentials(
        &self,
        cancel: &Cancellation,
    ) -> Result<std::sync::Arc<Credentials>> {
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let state = self.lock();
            let key = endpoint(&state.settings);
            if let Some(credentials) = state.pairing.credentials.get(&key) {
                return Ok(credentials.clone());
            }
            if !state.pairing.restoring.contains(&key) || Instant::now() >= until {
                return Err(Error::Authentication);
            }
            drop(state);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, net::TcpListener};

    #[tokio::test]
    async fn cancelling_a_pending_challenge_closes_the_socket_and_rejects_late_results() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (arrived, wait) = tokio::sync::oneshot::channel();
        let task = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                socket.read_exact(&mut b).unwrap();
                headers.push(b[0]);
                assert!(headers.len() < 8192);
            }
            arrived.send(()).unwrap();
            // Do not answer: a cancelled challenge must release the connection.
            assert_eq!(socket.read(&mut [0]).unwrap(), 0);
        });
        let shared = crate::casting::Shared::default();
        shared.begin_pairing(
            crate::casting::Settings {
                host: address.ip().to_string(),
                port: address.port(),
                allow_lan: true,
                ..Default::default()
            },
            true,
        );
        tokio::time::timeout(Duration::from_secs(2), wait)
            .await
            .unwrap()
            .unwrap();
        let attempt = shared.pairing_status().attempt;
        shared.cancel_pairing(attempt);
        shared.finish_pairing(attempt, Pin("0427".into()));
        tokio::task::spawn_blocking(move || task.join().unwrap())
            .await
            .unwrap();
        assert_eq!(shared.pairing_status().stage, Stage::Idle);
        assert!(shared.lock().pairing.credentials.is_empty());
    }

    #[tokio::test]
    async fn remembered_pairing_is_scoped_to_the_endpoint_and_does_not_ask_for_a_pin_again() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let settings = crate::casting::Settings {
            host: address.ip().to_string(),
            port: address.port(),
            allow_lan: true,
            ..Default::default()
        };
        let shared = crate::casting::Shared::default();
        let credentials = Credentials {
            seed: [1; 32],
            receiver_key: [2; 32],
            receiver_id: b"fixture".to_vec(),
            client_id: b"client".to_vec(),
        };
        let encoded = serde_json::to_string(&credentials).unwrap();
        let decoded: Credentials = serde_json::from_str(&encoded).unwrap();
        assert!(decoded.valid());
        shared
            .lock()
            .pairing
            .credentials
            .insert(endpoint(&settings), std::sync::Arc::new(decoded));
        shared.begin_pairing(settings.clone(), false);
        tokio::time::timeout(Duration::from_secs(1), async {
            while shared.pairing_status().stage != Stage::Paired {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            listener.accept().is_err(),
            "A remembered pairing must not display a new PIN"
        );
        let mut other = settings;
        other.port += 1;
        assert!(
            !shared
                .lock()
                .pairing
                .credentials
                .contains_key(&endpoint(&other))
        );
    }
}
