// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::Error;
use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr},
};

#[cfg(unix)]
pub(crate) struct LocalInterface {
    pub(crate) index: Option<u32>,
    pub(crate) address: IpAddr,
    pub(crate) multicast: bool,
    pub(crate) up: bool,
}

#[cfg(unix)]
pub(crate) fn eligible_interfaces(records: Vec<LocalInterface>) -> Vec<Ipv4Addr> {
    let mut seen = HashSet::new();
    let mut addresses = Vec::new();
    for record in records {
        let Some(index) = record.index else { continue };
        // Native deduplicates interface indices before family/flag filtering.
        if !seen.insert(index) {
            continue;
        }
        if let IpAddr::V4(ip) = record.address
            && ip != Ipv4Addr::LOCALHOST
            && record.multicast
            && record.up
            && !addresses.contains(&ip)
        {
            addresses.push(ip);
        }
    }
    if addresses.is_empty() {
        addresses.push(Ipv4Addr::LOCALHOST);
    }
    addresses
}

#[cfg(unix)]
fn local_interface_records() -> Result<Vec<LocalInterface>, Error> {
    use nix::{
        ifaddrs::getifaddrs,
        net::if_::{InterfaceFlags, if_nametoindex},
    };
    let mut records = Vec::new();
    for interface in getifaddrs().map_err(|error| Error::Native(error.to_string()))? {
        let address = interface.address.and_then(|address| {
            address
                .as_sockaddr_in()
                .map(|v4| IpAddr::V4(v4.ip()))
                .or_else(|| address.as_sockaddr_in6().map(|v6| IpAddr::V6(v6.ip())))
        });
        let Some(address) = address else { continue };
        if interface.interface_name.contains('\u{fffd}') {
            return Err(Error::Unsupported("ambiguous interface name encoding"));
        }
        records.push(LocalInterface {
            index: if_nametoindex(interface.interface_name.as_str()).ok(),
            address,
            multicast: interface.flags.contains(InterfaceFlags::IFF_MULTICAST),
            up: interface.flags.contains(InterfaceFlags::IFF_UP),
        });
    }
    Ok(records)
}

pub(crate) fn local_interfaces() -> Result<Vec<Ipv4Addr>, Error> {
    Ok(eligible_interfaces(local_interface_records()?))
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn native_private(address: Ipv4Addr) -> bool {
    let [a, b, _, _] = address.octets();
    a == 10 || (a == 192 && b == 168) || (a == 169 && b == 254)
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn select_interface(records: Vec<LocalInterface>, first_a: Option<Ipv4Addr>) -> Ipv4Addr {
    let interfaces = eligible_interfaces(records);
    first_a
        .filter(|address| {
            // Native prefixes on canonical IPv4 text; not RFC private/loopback.
            !native_private(*address)
                && address.octets()[..2] != [127, 0]
                && interfaces.contains(address)
        })
        .unwrap_or_else(|| {
            interfaces
                .iter()
                .copied()
                .find(|address| !native_private(*address))
                .unwrap_or(interfaces[0])
        })
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn first_a(
    hostname: nix::Result<std::ffi::OsString>,
    lookup: impl FnOnce(&std::ffi::CStr) -> Result<Option<Ipv4Addr>, gz_transport_nss::Error>,
) -> Result<Option<Ipv4Addr>, Error> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let Ok(hostname) = hostname else {
        return Ok(None);
    };
    if hostname.is_empty() || hostname == "localhost" {
        return Ok(None);
    }
    let hostname = CString::new(hostname.as_bytes())
        .map_err(|_| Error::Native("NUL in system hostname".into()))?;
    match lookup(&hostname) {
        Ok(address) => Ok(address),
        // Native failed gethostbyname falls back to its interface inventory.
        Err(gz_transport_nss::Error::Lookup { .. }) => Ok(None),
        Err(gz_transport_nss::Error::BufferLimit) => {
            Err(Error::Unsupported("resolver scratch limit"))
        }
        Err(error) => Err(Error::Native(format!("interface resolver: {error:?}"))),
    }
}

/// Explicit caller setup only. NSS may block; this is never worker/tick work.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(crate) fn automatic_interface() -> Result<Ipv4Addr, Error> {
    let candidate = first_a(
        nix::unistd::gethostname(),
        gz_transport_nss::lookup_first_v4,
    )?;
    Ok(select_interface(local_interface_records()?, candidate))
}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
#[path = "../tests/support/interfaces.rs"]
mod tests;
