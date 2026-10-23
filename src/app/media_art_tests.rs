//! Exercise the artwork GNOME consumes over an isolated MPRIS session bus.
use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use zbus::zvariant::OwnedValue;

type Metadata = HashMap<String, OwnedValue>;
fn text<'a>(metadata: &'a Metadata, key: &str) -> Option<&'a str> {
    metadata
        .get(key)
        .and_then(|value| <&str>::try_from(value).ok())
}

#[test]
fn mpris_server_art_is_a_readable_file_and_updates_without_changing_tracks() {
    const CHILD: &str = "SONICFAST_MPRIS_ART_TEST";
    const ROOT: &str = "SONICFAST_MPRIS_ART_ROOT";
    if std::env::var_os(CHILD).is_none() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(format!("mpris-art-{:016x}", rand::random::<u64>()));
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("session.conf");
        std::fs::write(&config, r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>"#).unwrap();
        let result = std::process::Command::new("dbus-run-session")
            .arg("--config-file").arg(config).arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "app::media_art_tests::mpris_server_art_is_a_readable_file_and_updates_without_changing_tracks", "--nocapture"])
            .env(CHILD, "1").env(ROOT, &root).output().unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 150, 80, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let release = Arc::new(AtomicBool::new(false));
    let requests = Arc::new(AtomicUsize::new(0));
    let server_release = release.clone();
    let count = requests.clone();
    let image = png.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        for _ in 0..2 {
            let mut socket = loop {
                if let Ok((socket, _)) = listener.accept() {
                    break socket;
                }
                assert!(Instant::now() < deadline, "Artwork was never requested");
                std::thread::sleep(Duration::from_millis(5));
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 8192);
            }
            // Only the app's authenticated adapter talks to this fixture.
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("GET /rest/getCoverArt"));
            assert!(request.contains("u=fixture") && request.contains("t="));
            count.fetch_add(1, Ordering::SeqCst);
            while !server_release.load(Ordering::SeqCst) {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", image.len()).as_bytes()).unwrap();
            socket.write_all(&image).unwrap();
        }
    });
    let mut app = App::new_subsonic(
        &Waker::default(),
        AppDirs {
            config: root.join("config"),
            state: root.join("state"),
            cache: root.join("cache # artwork"),
        },
        Settings::default(),
        AppOptions {
            restore_sign_in: false,
            media_controls: false,
            tray: false,
            autostart: false,
        },
    );
    app.local_ready = true;
    let client = libresonic::opensubsonic::Client::new(
        libresonic::ServerId::new("fixture").unwrap(),
        &endpoint,
        libresonic::opensubsonic::Credentials::token("fixture", "fixture-only-secret"),
        libresonic::opensubsonic::ClientOptions::default().allow_http(true),
    )
    .unwrap();
    app.backend.art().set_subsonic(Some(Arc::new(client)));
    app.media_controls = Some(MediaControls::start(
        fastframe_now_playing::App::new("sonicfast-art-test", "Sonicfast"),
        || {},
    ));
    let bus = zbus::blocking::connection::Builder::session()
        .unwrap()
        .method_timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let proxy = zbus::blocking::Proxy::new(
        &bus,
        "org.mpris.MediaPlayer2.sonicfast-art-test",
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .unwrap();
    let artwork = |name| {
        let id = libresonic::MediaId::new(
            libresonic::ServerId::new("fixture").unwrap(),
            libresonic::MediaKind::Artwork,
            name,
        )
        .unwrap();
        crate::subsonic::adapter::images(Some(&id))[0].url.clone()
    };
    let set_track = |app: &mut App, name: &str, art: Option<String>| {
        app.local = LocalState {
            connected: true,
            playback: Playback::Paused,
            track: Some(crate::player::LocalTrack {
                uri: format!("sonicfast:track:{name}"),
                title: name.into(),
                art_url: art,
                ..Default::default()
            }),
            ..Default::default()
        };
    };
    let ctx = egui::Context::default();
    set_track(&mut app, "First", Some(artwork("first")));
    let read = |app: &mut App, title: &str, want_art: bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.sync_media_controls(&ctx);
            if let Ok(metadata) = proxy.get_property::<Metadata>("Metadata")
                && text(&metadata, "xesam:title") == Some(title)
                && text(&metadata, "mpris:artUrl").is_some() == want_art
            {
                break metadata;
            }
            assert!(
                Instant::now() < deadline,
                "MPRIS artwork did not update for {title}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    // The initial metadata must omit an unresolved internal ID. When the
    // asynchronous download lands, the same track gets a PropertiesChanged update.
    read(&mut app, "First", false);
    release.store(true, Ordering::SeqCst);
    let first = read(&mut app, "First", true);
    let check = |metadata: &Metadata| {
        let uri = text(metadata, "mpris:artUrl").unwrap();
        assert!(uri.starts_with("file://"));
        assert!(
            !uri.contains("sonic-art:")
                && !uri.contains("fixture-only-secret")
                && !uri.contains(&endpoint)
        );
        assert!(
            uri.contains("%20") && uri.contains("%23"),
            "File URI must escape the cache directory"
        );
        let path = reqwest::Url::parse(uri).unwrap().to_file_path().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), png);
        assert_eq!(
            image::load_from_memory(&std::fs::read(&path).unwrap())
                .unwrap()
                .width(),
            8
        );
        uri.to_owned()
    };
    let first_uri = check(&first);
    set_track(&mut app, "Second", Some(artwork("second")));
    let second_uri = check(&read(&mut app, "Second", true));
    assert_ne!(
        first_uri, second_uri,
        "Track changes must not keep stale cover art"
    );
    set_track(&mut app, "No cover", None);
    read(&mut app, "No cover", false);
    assert_eq!(
        requests.load(Ordering::SeqCst),
        2,
        "Reuse cached artwork instead of asking the server every frame"
    );
    server.join().unwrap();
    app.backend.shutdown();
}
