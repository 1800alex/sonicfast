use super::{Kind, Receiver};
use mdns_sd::ResolvedService;
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    time::{Duration, Instant},
};
/// An explicitly requested, bounded mDNS scan. Does not connect to receivers.
pub fn discover() -> Result<Vec<Receiver>, String> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|_| "Cannot browse this network")?;
    let result = (|| {
        let air = daemon
            .browse("_raop._tcp.local.")
            .map_err(|_| "Cannot browse AirPlay receivers")?;
        let modern = daemon
            .browse("_airplay._tcp.local.")
            .map_err(|_| "Cannot browse AirPlay receivers")?;
        let cast = daemon
            .browse("_googlecast._tcp.local.")
            .map_err(|_| "Cannot browse Chromecast receivers")?;
        let mpd = daemon
            .browse("_mpd._tcp.local.")
            .map_err(|_| "Cannot browse MPD servers")?;
        let until = Instant::now() + Duration::from_secs(4);
        let mut found = BTreeMap::new();
        while Instant::now() < until {
            for (events, kind, modern) in [
                (&air, Kind::AirPlay, false),
                (&modern, Kind::AirPlay, true),
                (&cast, Kind::Chromecast, false),
                (&mpd, Kind::Mpd, false),
            ] {
                if let Ok(mdns_sd::ServiceEvent::ServiceResolved(service)) =
                    events.recv_timeout(Duration::from_millis(30))
                    && let Some(receiver) = classify(&service, kind, modern)
                {
                    merge(&mut found, receiver);
                }
            }
        }
        Ok(found.into_values().collect())
    })();
    let _ = daemon.shutdown();
    result
}

/// MPD's default `Music Player @ host` name (some builds add `[pid]`) as just
/// the host. A name the owner chose is shown as it is.
fn mpd_name(name: &str) -> &str {
    let Some(host) = name
        .strip_prefix("Music Player @ ")
        .filter(|h| !h.is_empty())
    else {
        return name;
    };
    match host.strip_suffix(']').and_then(|h| h.rsplit_once('[')) {
        Some((host, pid)) if !host.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => host,
    }
}
fn bits(value: Option<&str>) -> u64 {
    value
        .and_then(|v| u64::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok())
        .unwrap_or(0)
}
fn features(value: Option<&str>) -> u64 {
    let mut parts = value.unwrap_or("0").split(',');
    let low = bits(parts.next()) & 0xffffffff;
    low | ((bits(parts.next()) & 0xffffffff) << 32)
}
fn classify(service: &ResolvedService, kind: Kind, modern: bool) -> Option<Receiver> {
    let prop = |key| service.get_property_val_str(key);
    let name = prop("fn").or_else(|| prop("name")).unwrap_or_else(|| {
        service
            .get_fullname()
            .split("._")
            .next()
            .unwrap_or("Receiver")
    });
    // RAOP instances are named `MAC@Name`.
    let name = match kind {
        Kind::AirPlay => name.split_once('@').map_or(name, |(_, v)| v),
        Kind::Mpd => mpd_name(name),
        _ => name,
    };
    let name = name
        .chars()
        .filter(|c| !c.is_control())
        .take(128)
        .collect::<String>();
    // An announcement from this computer also lists loopback; prefer the LAN.
    let addresses: Vec<_> = service
        .get_addresses()
        .iter()
        .map(|v| v.to_ip_addr())
        .filter(|v| v.is_ipv4())
        .collect();
    let ip = addresses
        .iter()
        .find(|v| !v.is_loopback())
        .or(addresses.first())
        .copied()?;
    let flags = bits(prop("sf").or_else(|| prop("flags")));
    let ft = features(prop("ft").or_else(|| prop("features")));
    let airplay2 = kind == Kind::AirPlay && (modern || ft & ((1 << 38) | (1 << 48)) != 0);
    let reason = if kind != Kind::AirPlay {
        None
    } else if prop("pw").is_some_and(|v| v.eq_ignore_ascii_case("true")) || flags & 0x80 != 0 {
        Some("AirPlay passwords are not supported; this differs from an on-screen pairing code")
    } else if prop("acl") == Some("1") || prop("act") == Some("2") {
        Some("This receiver restricts access to its Apple Home members")
    } else if modern && ft & ((1 << 9) | (1 << 40)) == 0 {
        Some("This AirPlay receiver does not advertise audio support")
    } else if !airplay2 && prop("et").is_some_and(|v| !v.split(',').any(|v| v.trim() == "0")) {
        Some("This Classic receiver requires unsupported RSA/FairPlay encryption")
    } else if !airplay2 && flags & 0x208 != 0 {
        Some("This receiver requires legacy AirPlay PIN pairing")
    } else {
        None
    };
    Some(Receiver {
        kind,
        name,
        address: SocketAddr::new(ip, service.get_port()),
        supported: reason.is_none(),
        pairing: airplay2,
        airplay2,
        reason: reason.map(str::to_owned),
    })
}
fn merge(found: &mut BTreeMap<String, Receiver>, receiver: Receiver) {
    // A receiver may announce both services, at different ports. Prefer its
    // usable Classic route, with one picker entry for the AirPlay/RAOP pair.
    let key = format!(
        "{:?}-{}-{}",
        receiver.kind,
        receiver.address.ip(),
        receiver.name
    );
    if found
        .get(&key)
        .is_some_and(|old| old.supported && (!old.airplay2 || !receiver.supported))
    {
        return;
    }
    if found.len() < 128 || found.contains_key(&key) {
        found.insert(key, receiver);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn service(modern: bool, properties: &[(&str, &str)]) -> ResolvedService {
        mdns_sd::ServiceInfo::new(
            if modern {
                "_airplay._tcp.local."
            } else {
                "_raop._tcp.local."
            },
            "001122334455@Fixture",
            "fixture.local.",
            "192.0.2.12",
            if modern { 7000 } else { 5000 },
            properties,
        )
        .unwrap()
        .as_resolved_service()
    }
    #[test]
    fn roku_airplay_only_advertisement_is_visible_and_requires_pin() {
        let info = service(
            true,
            &[
                ("features", "0x7F8AD0,0x38BCF46"),
                ("flags", "0x244"),
                ("model", "4640X"),
            ],
        );
        let receiver = classify(&info, Kind::AirPlay, true).unwrap();
        assert!(receiver.supported && receiver.pairing && receiver.airplay2);
        assert_eq!(receiver.name, "Fixture");
        assert_eq!(receiver.address.port(), 7000);
        assert_eq!(features(Some("0x7F8AD0,0x38BCF46")), 0x038bcf46007f8ad0);
    }
    #[test]
    fn an_mpd_server_is_listed_by_its_announced_name() {
        let info = mdns_sd::ServiceInfo::new(
            "_mpd._tcp.local.",
            "Music Player @ pi",
            "pi.local.",
            "192.0.2.30",
            6600,
            &[] as &[(&str, &str)],
        )
        .unwrap()
        .as_resolved_service();
        let receiver = classify(&info, Kind::Mpd, false).unwrap();
        assert_eq!(receiver.kind, Kind::Mpd);
        assert_eq!(
            receiver.name, "pi",
            "MPD's default name shows just the host"
        );
        assert_eq!(receiver.address, "192.0.2.30:6600".parse().unwrap());
        assert!(receiver.supported && !receiver.pairing);
    }
    #[test]
    fn mpd_default_names_show_the_host_and_custom_names_stay() {
        for (announced, shown) in [
            ("Music Player @ rpi-mpd-office[667]", "rpi-mpd-office"),
            ("Music Player @ alex-desktop", "alex-desktop"),
            ("Kitchen speakers", "Kitchen speakers"),
            ("Music Player @ ", "Music Player @ "),
            ("Den [stereo]", "Den [stereo]"),
        ] {
            assert_eq!(mpd_name(announced), shown);
        }
    }
    #[test]
    fn a_lan_address_is_preferred_over_loopback() {
        let info = mdns_sd::ServiceInfo::new(
            "_mpd._tcp.local.",
            "Music Player @ desktop[11108]",
            "desktop.local.",
            &["127.0.0.1", "192.0.2.15"][..],
            6600,
            &[] as &[(&str, &str)],
        )
        .unwrap()
        .as_resolved_service();
        let receiver = classify(&info, Kind::Mpd, false).unwrap();
        assert_eq!(receiver.address, "192.0.2.15:6600".parse().unwrap());
        assert_eq!(receiver.name, "desktop");
    }
    #[test]
    fn passwords_home_only_and_video_only_are_not_pin_receivers() {
        for properties in [
            vec![("pw", "TRUE")],
            vec![("flags", "0x80")],
            vec![("acl", "1")],
            vec![("act", "2")],
            vec![("features", "0x1")],
        ] {
            let receiver = classify(&service(true, &properties), Kind::AirPlay, true).unwrap();
            assert!(!receiver.supported && receiver.reason.is_some());
        }
    }
    #[test]
    fn dual_advertisements_merge_in_either_order_and_preserve_classic() {
        let classic = classify(&service(false, &[("et", "0,1")]), Kind::AirPlay, false).unwrap();
        let modern = classify(
            &service(true, &[("features", "0x200")]),
            Kind::AirPlay,
            true,
        )
        .unwrap();
        for pair in [[classic.clone(), modern.clone()], [modern, classic.clone()]] {
            let mut found = BTreeMap::new();
            for receiver in pair {
                merge(&mut found, receiver);
            }
            assert_eq!(found.len(), 1);
            assert_eq!(found.into_values().next().unwrap(), classic);
        }
        let encrypted = classify(&service(false, &[("et", "1")]), Kind::AirPlay, false).unwrap();
        assert!(!encrypted.supported);
    }
}
