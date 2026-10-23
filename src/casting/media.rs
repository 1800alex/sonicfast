//! Per-track bearer leases over a bounded, receiver-restricted HTTP server.
use libresonic::playback::Cancellation;
use libresonic::{MediaId, StreamProvider};
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};
#[derive(Clone)]
struct Grant {
    path: String,
    id: MediaId,
    mime: String,
    expires: Instant,
    cancel: Cancellation,
}
pub(super) struct Server {
    address: SocketAddr,
    grant: Arc<Mutex<Option<Grant>>>,
    cancel: Cancellation,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    pub async fn new(
        provider: Arc<dyn StreamProvider>,
        local: IpAddr,
        peer: IpAddr,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind(SocketAddr::new(local, 0))
            .await
            .map_err(|_| "Cannot open the Cast media listener")?;
        let address = listener
            .local_addr()
            .map_err(|_| "Cannot open the Cast media listener")?;
        let grant: Arc<Mutex<Option<Grant>>> = Arc::default();
        let current = grant.clone();
        let cancel = Cancellation::default();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            let limit = Arc::new(Semaphore::new(4));
            loop {
                tokio::select! {
                    _=token.cancelled()=>break,
                    accepted=listener.accept()=>{
                        let Ok((socket,address))=accepted else{break;};
                        if address.ip()!=peer {drop(socket);continue;}
                        let Ok(permit)=limit.clone().try_acquire_owned() else{drop(socket);continue;};
                        let grant=current.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
                        let provider=provider.clone();let token=token.clone();
                        tokio::spawn(async move {let _permit=permit;tokio::select! {
                            _=token.cancelled()=>{},
                            _=tokio::time::timeout(Duration::from_secs(14400),serve(socket,provider,grant))=>{},
                        }});
                    }
                }
            }
        });
        Ok(Self {
            address,
            grant,
            cancel,
            task,
        })
    }
    pub fn issue(&self, id: MediaId, mime: String) -> String {
        self.revoke();
        let path = format!(
            "/media/{:032x}{:032x}",
            rand::random::<u128>(),
            rand::random::<u128>()
        );
        *self
            .grant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Grant {
            path: path.clone(),
            id,
            mime,
            expires: Instant::now() + Duration::from_secs(14400),
            cancel: Cancellation::default(),
        });
        format!("http://{}{path}", self.address)
    }
    pub fn revoke(&self) {
        if let Some(old) = self
            .grant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            old.cancel.cancel();
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.revoke();
        self.cancel.cancel();
        self.task.abort();
    }
}
async fn serve(
    mut socket: TcpStream,
    provider: Arc<dyn StreamProvider>,
    grant: Option<Grant>,
) -> Result<(), ()> {
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !bytes.ends_with(b"\r\n\r\n") {
            if bytes.len() >= 8192 {
                return Err(());
            }
            let b = socket.read_u8().await.map_err(|_| ())?;
            bytes.push(b);
        }
        Ok(())
    })
    .await
    .map_err(|_| ())??;
    let request = std::str::from_utf8(&bytes).map_err(|_| ())?;
    let mut line = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = line.next().unwrap_or_default();
    let path = line.next().unwrap_or_default();
    let Some(grant) =
        grant.filter(|g| g.path == path && g.expires > Instant::now() && !g.cancel.is_cancelled())
    else {
        return reply(&mut socket, "404 Not Found", "").await;
    };
    if !matches!(method, "GET" | "HEAD" | "OPTIONS") {
        return reply(&mut socket, "405 Method Not Allowed", "").await;
    }
    if method == "OPTIONS" {
        return reply(&mut socket,"204 No Content","Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\nAccess-Control-Allow-Headers: Range\r\nAccess-Control-Allow-Private-Network: true\r\n").await;
    }
    let operation = async {
        let mut stream = tokio::time::timeout(Duration::from_secs(10), provider.open(&grant.id))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
        let total = stream.byte_len();
        let range = request.lines().skip(1).find_map(|line| {
            line.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("range"))
                .map(|(_, v)| v.trim())
        });
        let (start, end) = match range {
            Some(range) => match total.and_then(|n| parse_range(range, n)) {
                Some(v) if stream.is_seekable() || v.0 == 0 => v,
                _ => {
                    return reply(
                        &mut socket,
                        "416 Range Not Satisfiable",
                        &total.map_or(String::new(), |n| format!("Content-Range: bytes */{n}\r\n")),
                    )
                    .await;
                }
            },
            None => (0, total.map(|n| n.saturating_sub(1))),
        };
        if start > 0 {
            tokio::time::timeout(Duration::from_secs(10), stream.seek_to(start))
                .await
                .map_err(|_| ())?
                .map_err(|_| ())?;
        }
        let length = total.map(|n| {
            if n == 0 {
                0
            } else {
                end.unwrap_or(n - 1) - start + 1
            }
        });
        let mut headers = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nConnection: close\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Expose-Headers: Content-Length, Content-Range, Accept-Ranges\r\n",
            if range.is_some() {
                "206 Partial Content"
            } else {
                "200 OK"
            },
            grant.mime
        );
        if stream.is_seekable() && total.is_some() {
            headers.push_str("Accept-Ranges: bytes\r\n");
        }
        if let Some(n) = length {
            headers.push_str(&format!("Content-Length: {n}\r\n"));
        }
        if range.is_some() {
            headers.push_str(&format!(
                "Content-Range: bytes {start}-{}/{}\r\n",
                end.unwrap(),
                total.unwrap()
            ));
        }
        headers.push_str("\r\n");
        socket.write_all(headers.as_bytes()).await.map_err(|_| ())?;
        if method == "HEAD" {
            return Ok(());
        }
        let mut remaining = length.unwrap_or(u64::MAX);
        while remaining > 0 {
            let Some(bytes) = tokio::time::timeout(Duration::from_secs(10), stream.next())
                .await
                .map_err(|_| ())?
                .map_err(|_| ())?
            else {
                break;
            };
            if bytes.is_empty() || bytes.len() > libresonic::STREAM_CHUNK_BYTES {
                return Err(());
            }
            let n = bytes.len().min(remaining.min(usize::MAX as u64) as usize);
            tokio::time::timeout(Duration::from_secs(5), socket.write_all(&bytes[..n]))
                .await
                .map_err(|_| ())?
                .map_err(|_| ())?;
            remaining -= n as u64;
        }
        Ok(())
    };
    tokio::select! {result=operation=>result,_=grant.cancel.cancelled()=>Ok(()),_=tokio::time::sleep(grant.expires.saturating_duration_since(Instant::now()))=>Ok(())}
}
async fn reply(socket: &mut TcpStream, status: &str, headers: &str) -> Result<(), ()> {
    socket.write_all(format!("HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: 0\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n{headers}\r\n").as_bytes()).await.map_err(|_|())
}
fn parse_range(value: &str, total: u64) -> Option<(u64, Option<u64>)> {
    if total == 0 {
        return None;
    }
    let (a, b) = value.strip_prefix("bytes=")?.split_once('-')?;
    if a.is_empty() {
        let suffix = b.parse::<u64>().ok()?.min(total);
        return (suffix > 0).then_some((total - suffix, Some(total - 1)));
    }
    let start = a.parse::<u64>().ok()?;
    let end = if b.is_empty() {
        total - 1
    } else {
        b.parse::<u64>().ok()?.min(total - 1)
    };
    (start <= end && start < total).then_some((start, Some(end)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use libresonic::{AudioStream, MediaKind, ServerId, ServiceFuture};
    struct Memory;
    struct Bytes(usize);
    impl StreamProvider for Memory {
        fn open<'a>(&'a self, _: &'a MediaId) -> ServiceFuture<'a, Box<dyn AudioStream>> {
            Box::pin(async { Ok(Box::new(Bytes(0)) as Box<dyn AudioStream>) })
        }
    }
    impl AudioStream for Bytes {
        fn byte_len(&self) -> Option<u64> {
            Some(10)
        }
        fn is_seekable(&self) -> bool {
            true
        }
        fn seek_to(&mut self, offset: u64) -> ServiceFuture<'_, ()> {
            Box::pin(async move {
                self.0 = offset as usize;
                Ok(())
            })
        }
        fn next(&mut self) -> ServiceFuture<'_, Option<Vec<u8>>> {
            Box::pin(async move {
                if self.0 >= 10 {
                    return Ok(None);
                }
                let v = b"0123456789"[self.0..].to_vec();
                self.0 = 10;
                Ok(Some(v))
            })
        }
    }
    fn id() -> MediaId {
        MediaId::new(ServerId::new("test").unwrap(), MediaKind::Track, "one").unwrap()
    }
    async fn get(url: &str, range: Option<&str>, method: &str) -> String {
        let url = url.strip_prefix("http://").unwrap();
        let (address, path) = url.split_once('/').unwrap();
        let mut socket = TcpStream::connect(address).await.unwrap();
        socket
            .write_all(
                format!(
                    "{method} /{path} HTTP/1.1\r\nHost: {address}\r\n{}\r\n",
                    range.map_or(String::new(), |r| format!("Range: {r}\r\n"))
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut data = Vec::new();
        socket.read_to_end(&mut data).await.unwrap();
        String::from_utf8(data).unwrap()
    }
    #[tokio::test]
    async fn receiver_gets_ranges_head_and_cors_but_old_and_forged_leases_are_revoked() {
        let server = Server::new(
            Arc::new(Memory),
            "127.0.0.1".parse().unwrap(),
            "127.0.0.1".parse().unwrap(),
        )
        .await
        .unwrap();
        let url = server.issue(id(), "audio/mpeg".into());
        let response = get(&url, Some("bytes=3-6"), "GET").await;
        assert!(response.starts_with("HTTP/1.1 206"));
        assert!(response.contains("Content-Range: bytes 3-6/10"));
        assert!(response.ends_with("\r\n\r\n3456"));
        assert!(
            get(&url, Some("bytes=-2"), "GET")
                .await
                .ends_with("\r\n\r\n89")
        );
        assert!(get(&url, None, "HEAD").await.ends_with("\r\n\r\n"));
        assert!(
            get(&url, None, "OPTIONS")
                .await
                .contains("Access-Control-Allow-Private-Network: true")
        );
        for range in [
            "bytes=50-",
            "bytes=2-1",
            "bytes=0-1,3-4",
            "bytes=-0",
            "bytes=18446744073709551616-",
        ] {
            assert!(
                get(&url, Some(range), "GET")
                    .await
                    .starts_with("HTTP/1.1 416")
            );
        }
        let next = server.issue(id(), "audio/flac".into());
        assert!(get(&url, None, "GET").await.starts_with("HTTP/1.1 404"));
        assert!(
            get(&(next.clone() + "x"), None, "GET")
                .await
                .starts_with("HTTP/1.1 404")
        );
        assert!(get(&next, None, "POST").await.starts_with("HTTP/1.1 405"));
        server.revoke();
        assert!(get(&next, None, "GET").await.starts_with("HTTP/1.1 404"));
    }
    #[tokio::test]
    async fn media_server_rejects_another_peer_even_with_the_token() {
        let server = Server::new(
            Arc::new(Memory),
            "127.0.0.1".parse().unwrap(),
            "127.0.0.2".parse().unwrap(),
        )
        .await
        .unwrap();
        let url = server.issue(id(), "audio/mpeg".into());
        let socket = TcpStream::connect(server.address).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut bytes = [0; 1];
        let mut socket = socket;
        assert_eq!(socket.read(&mut bytes).await.unwrap(), 0);
        assert!(!url.is_empty());
    }
}
