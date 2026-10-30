// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0

use super::*;
fn record(index: Option<u32>, address: &str, up: bool, multicast: bool) -> LocalInterface {
    LocalInterface {
        index,
        address: address.parse().unwrap(),
        up,
        multicast,
    }
}
fn rows(addresses: &[&str]) -> Vec<LocalInterface> {
    addresses
        .iter()
        .enumerate()
        .map(|(index, address)| record(Some(index as u32 + 1), address, true, true))
        .collect()
}
#[test]
fn preferred_can_reorder_but_must_be_eligible_and_native_public() {
    for (candidate, expected) in [
        (Some("203.0.113.9"), "203.0.113.9"),
        (Some("192.168.1.2"), "198.51.100.2"),
        (Some("203.0.113.99"), "198.51.100.2"),
        (None, "198.51.100.2"),
    ] {
        assert_eq!(
            select_interface(
                rows(&["192.168.1.2", "198.51.100.2", "203.0.113.9"]),
                candidate.map(|value| value.parse().unwrap())
            ),
            expected.parse::<Ipv4Addr>().unwrap()
        );
    }
}
#[test]
fn native_private_prefixes_and_preferred_loopback_prefix_are_distinct() {
    for (address, private) in [
        ("10.0.0.1", true),
        ("192.168.255.255", true),
        ("169.254.1.2", true),
        ("172.16.1.2", false),
        ("172.31.255.255", false),
        ("169.253.1.2", false),
        ("192.167.1.2", false),
        ("127.0.2.3", false),
        ("127.1.2.3", false),
    ] {
        assert_eq!(native_private(address.parse().unwrap()), private);
    }
    for (candidate, expected) in [("127.0.2.3", "198.51.100.2"), ("127.1.2.3", "127.1.2.3")] {
        assert_eq!(
            select_interface(
                rows(&["198.51.100.2", candidate]),
                Some(candidate.parse().unwrap())
            ),
            expected.parse::<Ipv4Addr>().unwrap()
        );
    }
    assert_eq!(
        select_interface(rows(&["127.0.2.3", "203.0.113.9"]), None),
        "127.0.2.3".parse::<Ipv4Addr>().unwrap()
    );
}
#[test]
fn rejected_records_still_claim_their_index() {
    for (address, up, multicast) in [
        ("::1", true, true),
        ("127.0.0.1", true, true),
        ("203.0.113.9", false, true),
        ("203.0.113.9", true, false),
    ] {
        let records = || {
            vec![
                record(Some(1), address, up, multicast),
                record(Some(1), "198.51.100.2", true, true),
                record(Some(2), "192.168.1.2", true, true),
            ]
        };
        assert_eq!(
            eligible_interfaces(records()),
            ["192.168.1.2".parse::<Ipv4Addr>().unwrap()]
        );
        assert_eq!(
            select_interface(records(), Some("198.51.100.2".parse().unwrap())),
            "192.168.1.2".parse::<Ipv4Addr>().unwrap()
        );
    }
}
#[test]
fn missing_index_address_dedup_and_fallback_preserve_order() {
    let inventory = || {
        let mut inventory = rows(&["10.1.2.3", "192.168.1.2", "10.1.2.3"]);
        inventory.insert(0, record(None, "203.0.113.9", true, true));
        inventory
    };
    assert_eq!(
        eligible_interfaces(inventory()),
        [
            "10.1.2.3".parse::<Ipv4Addr>().unwrap(),
            "192.168.1.2".parse().unwrap()
        ]
    );
    assert_eq!(
        select_interface(inventory(), None),
        "10.1.2.3".parse::<Ipv4Addr>().unwrap()
    );
    assert_eq!(select_interface(Vec::new(), None), Ipv4Addr::LOCALHOST);
}

#[test]
fn hostname_prerequisites_and_native_lookup_errors_fall_back() {
    for hostname in [
        Err(nix::errno::Errno::EIO),
        Ok("".into()),
        Ok("localhost".into()),
    ] {
        assert_eq!(
            first_a(hostname, |_| panic!("lookup must be skipped")),
            Ok(None)
        );
    }
    for result in [
        Ok(None),
        Err(gz_transport_nss::Error::Lookup {
            code: 11,
            host_error: 2,
        }),
    ] {
        assert_eq!(first_a(Ok("fixture".into()), |_| result), Ok(None));
    }
    assert_eq!(
        first_a(Ok("fixture".into()), |_| Ok(Some(Ipv4Addr::LOCALHOST))),
        Ok(Some(Ipv4Addr::LOCALHOST))
    );
}

#[test]
fn resource_errors_stay_visible_and_hostname_bytes_are_not_lossy() {
    use std::os::unix::ffi::OsStringExt;
    assert_eq!(
        first_a(Ok("fixture".into()), |_| Err(
            gz_transport_nss::Error::BufferLimit
        )),
        Err(Error::Unsupported("resolver scratch limit"))
    );
    for error in [
        gz_transport_nss::Error::Allocation,
        gz_transport_nss::Error::NameTooLong,
        gz_transport_nss::Error::MalformedResult,
    ] {
        assert_eq!(
            first_a(Ok("fixture".into()), |_| Err(error.clone())),
            Err(Error::Native(format!("interface resolver: {error:?}")))
        );
    }
    let raw = std::ffi::OsString::from_vec(vec![b'f', 255]);
    assert_eq!(
        first_a(Ok(raw), |name| {
            assert_eq!(name.to_bytes(), &[b'f', 255]);
            Ok(None)
        }),
        Ok(None)
    );
    assert_eq!(
        first_a(Ok(std::ffi::OsString::from_vec(vec![b'f', 0])), |_| panic!(
            "NUL rejected before lookup"
        )),
        Err(Error::Native("NUL in system hostname".into()))
    );
}
