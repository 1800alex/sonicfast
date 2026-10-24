//! Checks a Cast receiver's identity the way the output picker does and prints
//! the result as JSON, including the fingerprint it presented when unverified.
//! It never launches anything on the receiver.
//! Usage: cast_check HOST PORT [FINGERPRINT]
use sonicfast::casting::{Kind, Settings, Trust, check_cast};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host = args.first().ok_or("host required")?.clone();
    let port = args.get(1).ok_or("port required")?.parse()?;
    let settings = Settings {
        output: Some(Kind::Chromecast),
        host,
        port,
        name: "cast_check".into(),
        allow_lan: true,
        certificate_sha256: args.get(2).cloned().unwrap_or_default(),
        ..Default::default()
    };
    let result = match check_cast(&settings) {
        Trust::Verified => serde_json::json!({"trust": "verified"}),
        Trust::Untrusted { pinned, presented } => {
            serde_json::json!({"trust": "untrusted", "pinned": pinned, "presented": presented})
        }
        Trust::Unreachable => serde_json::json!({"trust": "unreachable"}),
    };
    println!("{result}");
    Ok(())
}
