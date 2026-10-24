//! Prints what LAN receivers advertise over mDNS, for diagnosing the output
//! picker's discovery. Passive: it never connects to a receiver.
//! Usage: cargo run --example mdns_browse -- [SERVICE ...] [--seconds N]
//! Default services: _mpd._tcp, _googlecast._tcp, _raop._tcp, _airplay._tcp.
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut seconds = 5;
    if let Some(at) = args.iter().position(|a| a == "--seconds") {
        seconds = args.get(at + 1).ok_or("--seconds needs a value")?.parse()?;
        args.drain(at..=at + 1);
    }
    if args.is_empty() {
        args = [
            "_mpd._tcp",
            "_googlecast._tcp",
            "_raop._tcp",
            "_airplay._tcp",
        ]
        .map(String::from)
        .to_vec();
    }
    let daemon = mdns_sd::ServiceDaemon::new()?;
    let browsers = args
        .iter()
        .map(|service| daemon.browse(&format!("{service}.local.")))
        .collect::<Result<Vec<_>, _>>()?;
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        for events in &browsers {
            if let Ok(mdns_sd::ServiceEvent::ServiceResolved(service)) =
                events.recv_timeout(Duration::from_millis(30))
            {
                let properties: std::collections::BTreeMap<_, _> = service
                    .get_properties()
                    .iter()
                    .map(|p| (p.key().to_owned(), p.val_str().to_owned()))
                    .collect();
                println!(
                    "{}",
                    serde_json::json!({
                        "fullname": service.get_fullname(),
                        "host": service.get_hostname(),
                        "port": service.get_port(),
                        "addresses": service.get_addresses().iter().map(|a| a.to_ip_addr().to_string()).collect::<Vec<_>>(),
                        "txt": properties,
                    })
                );
            }
        }
    }
    let _ = daemon.shutdown();
    Ok(())
}
