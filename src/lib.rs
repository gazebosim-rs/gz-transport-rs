// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
//! Gazebo's ZeroMQ transport. Each [`Node`] owns one bounded IO worker.
mod config;
mod names;
mod protocol;
mod runtime;

pub use config::Config;
pub use gz_msgs::discovery::discovery::publisher::Scope;
pub use names::{as_valid_topic, decompose_fully_qualified_topic, qualified_name};
pub use runtime::{
    AdvertiseOptions, Message, MessageInfo, Node, OneWayCallbackService, PendingResponse,
    Publisher, ReceivedMessage, Request, Service, ServiceProviderInfo, ServiceTypeResolution,
    ServiceTypes, Statistics, SubscribeOptions, Subscription, TopicEndpointTypes,
    TopicPublisherInfo,
};

/// Errors are explicit: no global executor, unbounded retry, or silent fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidName,
    InvalidConfig,
    AlreadyAdvertised,
    AlreadyRemapped,
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
