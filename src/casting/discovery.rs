use super::{Kind, Receiver};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
/// An explicitly requested, bounded mDNS scan. Does not connect to receivers.
pub fn discover() -> Result<Vec<Receiver>, String> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|_| "Cannot browse this network")?;
    let result = (|| {
        let air = daemon
            .browse("_raop._tcp.local.")
            .map_err(|_| "Cannot browse AirPlay receivers")?;
        let cast = daemon
            .browse("_googlecast._tcp.local.")
            .map_err(|_| "Cannot browse Chromecast receivers")?;
        let until = Instant::now() + Duration::from_secs(4);
        let mut found = BTreeMap::new();
        while Instant::now() < until {
            for (events, kind) in [(&air, Kind::AirPlay), (&cast, Kind::Chromecast)] {
                if let Ok(mdns_sd::ServiceEvent::ServiceResolved(service)) =
                    events.recv_timeout(Duration::from_millis(30))
                {
                    let name = service.get_property_val_str("fn").unwrap_or_else(|| {
                        service
                            .get_fullname()
                            .split("._")
                            .next()
                            .unwrap_or("Receiver")
                    });
                    let name = name
                        .split_once('@')
                        .map_or(name, |(_, v)| v)
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(128)
                        .collect::<String>();
                    let supported = kind != Kind::AirPlay
                        || (service
                            .get_property_val_str("et")
                            .is_none_or(|v| v.split(',').any(|v| v.trim() == "0"))
                            && service.get_property_val_str("pw") != Some("true"));
                    if let Some(ip) = service
                        .get_addresses()
                        .iter()
                        .map(|v| v.to_ip_addr())
                        .find(|v| v.is_ipv4())
                    {
                        let address = std::net::SocketAddr::new(ip, service.get_port());
                        if found.len() < 128 {
                            found.insert(
                                format!("{:?}-{address}", kind),
                                Receiver {
                                    kind,
                                    name,
                                    address,
                                    supported,
                                },
                            );
                        }
                    }
                }
            }
        }
        Ok(found.into_values().collect())
    })();
    let _ = daemon.shutdown();
    result
}
