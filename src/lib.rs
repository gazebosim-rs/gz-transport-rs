// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
//! Gazebo's ZeroMQ transport. Each [`Node`] owns one bounded IO worker.
mod config;
mod names;
mod protocol;
mod runtime;

pub use config::Config;
pub use names::{as_valid_topic, qualified_name};
pub use runtime::{
    Message, Node, Publisher, Request, Service, ServiceTypeResolution, ServiceTypes, Statistics,
    Subscription,
};

/// Errors are explicit: no global executor, unbounded retry, or silent fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidName,
    InvalidConfig,
    AlreadyAdvertised,
    Unsupported(&'static str),
    InvalidFrame,
    QueueFull,
    Shutdown,
    Timeout,
    Native(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<zmq::Error> for Error {
    fn from(value: zmq::Error) -> Self {
        Self::Native(value.to_string())
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Native(value.to_string())
    }
}
