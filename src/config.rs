// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::{Error, qualified_name};
use std::{
    collections::HashMap,
    net::Ipv4Addr,
    time::{Duration, Instant},
};

/// Explicit configuration. No process environment is read implicitly.
#[derive(Clone, Debug)]
pub struct Config {
    pub partition: String,
    pub namespace: String,
    pub interface: Ipv4Addr,
    pub multicast: Ipv4Addr,
    pub message_port: u16,
    pub service_port: u16,
    pub capacity: usize,
    pub send_high_watermark: usize,
    pub receive_high_watermark: usize,
    pub max_message_bytes: usize,
    pub reply_timeout: Duration,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            partition: String::new(),
            namespace: String::new(),
            interface: Ipv4Addr::LOCALHOST,
            multicast: Ipv4Addr::new(239, 255, 0, 7),
            message_port: 10317,
            service_port: 10318,
            capacity: 1000,
            send_high_watermark: 1000,
            receive_high_watermark: 1000,
            max_message_bytes: 16 * 1024 * 1024,
            reply_timeout: Duration::from_secs(5),
        }
    }
}

impl Config {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        qualified_name(&self.partition, &self.namespace, "validation")?;
        if self.capacity == 0
            || self.send_high_watermark == 0
            || self.send_high_watermark > i32::MAX as usize
            || self.receive_high_watermark == 0
            || self.receive_high_watermark > i32::MAX as usize
            || self.capacity > i32::MAX as usize
            || self.max_message_bytes == 0
            || self.max_message_bytes > i64::MAX as usize
            || self.reply_timeout.is_zero()
            || Instant::now().checked_add(self.reply_timeout).is_none()
            || !self.multicast.is_multicast()
            || self.interface.is_unspecified()
            || self.message_port == self.service_port
            || self.message_port == 0
            || self.service_port == 0
        {
            return Err(Error::InvalidConfig);
        }
        Ok(())
    }
}

const VARIABLES: &[&str] = &[
    "GZ_PARTITION",
    "GZ_IP",
    "GZ_DISCOVERY_MULTICAST_IP",
    "GZ_DISCOVERY_MSG_PORT",
    "GZ_DISCOVERY_SRV_PORT",
    "GZ_TRANSPORT_SNDHWM",
    "GZ_TRANSPORT_RCVHWM",
    "GZ_TRANSPORT_TOPIC_STATISTICS",
    "GZ_TRANSPORT_USERNAME",
    "GZ_TRANSPORT_PASSWORD",
    "GZ_RELAY",
    "GZ_TRANSPORT_IMPLEMENTATION",
];

impl Config {
    /// Capture recognized process configuration explicitly, before worker startup.
    /// Without GZ_PARTITION, Unix uses hostname:USER (UID account lookup if USER
    /// is absent). System identity lookup runs synchronously in the caller.
    pub fn from_env() -> Result<Self, Error> {
        let values = VARIABLES
            .iter()
            .filter_map(|&key| {
                std::env::var_os(key).map(|value| {
                    value
                        .into_string()
                        .map(|value| (key, value))
                        .map_err(|_| Error::InvalidConfig)
                })
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut config = Self::default();
        if !values.contains_key("GZ_PARTITION") {
            config.partition = default_partition()?;
        }
        config.with_environment(|key| values.get(key).cloned())
    }
    /// Apply Gazebo environment settings to an explicit base configuration.
    /// Unsupported wire modes are errors, so configuration cannot silently fall back.
    pub fn with_environment(
        mut self,
        mut lookup: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, Error> {
        if let Some(partition) = lookup("GZ_PARTITION") {
            self.partition = partition;
        }
        if let Some(ip) = lookup("GZ_IP").filter(|s| !s.is_empty()) {
            self.interface = ip.parse().map_err(|_| Error::InvalidConfig)?;
        }
        if let Some(ip) = lookup("GZ_DISCOVERY_MULTICAST_IP").filter(|s| !s.is_empty()) {
            self.multicast = ip.parse().map_err(|_| Error::InvalidConfig)?;
        }
        self.message_port = nonnegative(lookup("GZ_DISCOVERY_MSG_PORT"), self.message_port.into())
            .try_into()
            .map_err(|_| Error::InvalidConfig)?;
        self.service_port = nonnegative(lookup("GZ_DISCOVERY_SRV_PORT"), self.service_port.into())
            .try_into()
            .map_err(|_| Error::InvalidConfig)?;
        if self.message_port == self.service_port {
            self.service_port = if self.message_port < u16::MAX {
                self.service_port + 1
            } else {
                self.service_port - 1
            };
        }
        self.send_high_watermark = nonnegative(
            lookup("GZ_TRANSPORT_SNDHWM"),
            self.send_high_watermark
                .try_into()
                .map_err(|_| Error::InvalidConfig)?,
        ) as usize;
        self.receive_high_watermark = nonnegative(
            lookup("GZ_TRANSPORT_RCVHWM"),
            self.receive_high_watermark
                .try_into()
                .map_err(|_| Error::InvalidConfig)?,
        ) as usize;
        if self.send_high_watermark == 0 || self.receive_high_watermark == 0 {
            return Err(Error::Unsupported("unbounded ZeroMQ queues"));
        }
        if lookup("GZ_TRANSPORT_TOPIC_STATISTICS").is_some_and(|v| v == "1") {
            return Err(Error::Unsupported("statistics wire version 110"));
        }
        if lookup("GZ_RELAY").is_some_and(|v| !v.is_empty()) {
            return Err(Error::Unsupported("discovery relay"));
        }
        if let Some(implementation) =
            lookup("GZ_TRANSPORT_IMPLEMENTATION").filter(|v| !v.is_empty())
            && !implementation.eq_ignore_ascii_case("zeromq")
        {
            return Err(Error::Unsupported("transport implementation"));
        }
        let username = lookup("GZ_TRANSPORT_USERNAME");
        let password = lookup("GZ_TRANSPORT_PASSWORD");
        if username.is_some() && password.is_some() {
            return Err(Error::Unsupported("PLAIN authentication"));
        }
        self.validate()?;
        Ok(self)
    }
}

#[cfg(unix)]
fn default_partition() -> Result<String, Error> {
    use nix::unistd::{User, gethostname, getuid};
    partition_identity(gethostname(), std::env::var_os("USER"), || {
        User::from_uid(getuid()).map(|user| user.map(|user| user.name))
    })
}

#[cfg(not(unix))]
fn default_partition() -> Result<String, Error> {
    Err(Error::Unsupported(
        "automatic partition identity on this platform",
    ))
}

#[cfg(unix)]
fn partition_identity(
    hostname: nix::Result<std::ffi::OsString>,
    user: Option<std::ffi::OsString>,
    system_user: impl FnOnce() -> nix::Result<Option<String>>,
) -> Result<String, Error> {
    let hostname = hostname
        .map_err(|error| Error::Native(error.to_string()))?
        .into_string()
        .map_err(|_| Error::InvalidConfig)?;
    let username = if let Some(user) = user {
        user.into_string().map_err(|_| Error::InvalidConfig)?
    } else {
        let username = system_user()
            .map_err(|error| Error::Native(error.to_string()))?
            .ok_or(Error::InvalidConfig)?;
        // nix decodes NSS bytes lossily. Reject ambiguous replacement characters
        // rather than silently selecting a partition with a changed identity.
        if username.contains('\u{fffd}') {
            return Err(Error::Unsupported("ambiguous NSS username encoding"));
        }
        username
    };
    Ok(format!("{hostname}:{username}"))
}

// Preserve std::stoi's decimal prefix, ASCII leading whitespace and fallback.
fn nonnegative(value: Option<String>, default: i32) -> i32 {
    let Some(value) = value else {
        return default;
    };
    let value = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let prefix = value
        .bytes()
        .enumerate()
        .take_while(|(i, b)| b.is_ascii_digit() || (*i == 0 && matches!(b, b'+' | b'-')))
        .count();
    value[..prefix]
        .parse::<i32>()
        .ok()
        .filter(|v| *v >= 0)
        .unwrap_or(default)
}

#[cfg(test)]
#[path = "../tests/support/config.rs"]
mod tests;
