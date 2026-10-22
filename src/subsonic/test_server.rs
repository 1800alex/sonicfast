//! Synthetic local HTTP service used by download and reporting regressions.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[derive(Clone)]
pub struct Request {
    pub path: String,
    pub params: HashMap<String, String>,
}
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub audio: bool,
    pub stall: bool,
    pub declared: Option<usize>,
}
impl Reply {
    pub fn json(value: serde_json::Value) -> Self {
        Self {
            status: 200,
            body: serde_json::to_vec(&serde_json::json!({"subsonic-response":value})).unwrap(),
            audio: false,
            stall: false,
            declared: None,
        }
    }
    pub fn audio(bytes: Vec<u8>) -> Self {
        Self {
            status: 200,
            body: bytes,
            audio: true,
            stall: false,
            declared: None,
        }
    }
}
pub struct Server {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    pub async fn start(handler: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let received = requests.clone();
        let handler = Arc::new(handler);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let handler = handler.clone();
                let requests = received.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let mut chunk = [0; 8192];
                    while !bytes.windows(4).any(|s| s == b"\r\n\r\n") {
                        let n = socket.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 || bytes.len() > 16384 {
                            return;
                        }
                        bytes.extend_from_slice(&chunk[..n]);
                    }
                    let text = String::from_utf8_lossy(&bytes);
                    let Some(target) = text.split_whitespace().nth(1) else {
                        return;
                    };
                    let url = reqwest::Url::parse(&format!("http://fixture{target}")).unwrap();
                    let request = Request {
                        path: url.path().to_owned(),
                        params: url
                            .query_pairs()
                            .filter(|(k, _)| matches!(k.as_ref(), "id" | "time" | "submission"))
                            .map(|(k, v)| (k.into_owned(), v.into_owned()))
                            .collect(),
                    };
                    requests.lock().unwrap().push(request.clone());
                    let reply = handler(&request);
                    let header = format!(
                        "HTTP/1.1 {} Fixture\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        reply.status,
                        if reply.audio {
                            "audio/flac"
                        } else {
                            "application/json"
                        },
                        reply.declared.unwrap_or(reply.body.len())
                    );
                    if socket.write_all(header.as_bytes()).await.is_err() {
                        return;
                    }
                    if reply.stall {
                        let _ = socket
                            .write_all(reply.body.get(..64).unwrap_or(&reply.body))
                            .await;
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    } else {
                        let _ = socket.write_all(&reply.body).await;
                    }
                });
            }
        });
        Self {
            url,
            requests,
            task,
        }
    }
    pub fn client(&self) -> Arc<libresonic::opensubsonic::Client> {
        Arc::new(
            libresonic::opensubsonic::Client::new(
                libresonic::ServerId::new("fixture").unwrap(),
                &self.url,
                libresonic::opensubsonic::Credentials::token("fixture", "fixture-only"),
                libresonic::opensubsonic::ClientOptions::default().allow_http(true),
            )
            .unwrap(),
        )
    }
    pub fn count(&self, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.path == path)
            .count()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
