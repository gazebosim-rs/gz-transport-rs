// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::Error;
use gz_msgs::{
    discovery::{
        Discovery,
        discovery::{DiscContents, Publisher, Type},
    },
    prost::Message,
};

pub(crate) const VERSION: u32 = 10;
pub(crate) const MAX_DATAGRAM: usize = 65507;

pub(crate) fn discovery(process: &str, kind: Type, contents: Option<DiscContents>) -> Discovery {
    Discovery {
        version: VERSION,
        process_uuid: process.into(),
        r#type: kind as i32,
        disc_contents: contents,
        ..Default::default()
    }
}

pub(crate) fn encode(message: &Discovery) -> Result<Vec<u8>, Error> {
    if message.encoded_len() > MAX_DATAGRAM - 2 {
        return Err(Error::InvalidFrame);
    }
    let body = message.encode_to_vec();
    // The pinned upstream protocol uses a native-endian uint16 delimiter.
    let mut frame = (body.len() as u16).to_ne_bytes().to_vec();
    frame.extend(body);
    Ok(frame)
}

pub(crate) fn decode(frame: &[u8]) -> Result<Discovery, Error> {
    if frame.len() < 2
        || frame.len() > MAX_DATAGRAM
        || u16::from_ne_bytes([frame[0], frame[1]]) as usize != frame.len() - 2
    {
        return Err(Error::InvalidFrame);
    }
    let msg = Discovery::decode(&frame[2..]).map_err(|_| Error::InvalidFrame)?;
    if msg.version != VERSION {
        return Err(Error::InvalidFrame);
    }
    Ok(msg)
}

pub(crate) fn advertisement(process: &str, publisher: Publisher) -> Discovery {
    discovery(process, Type::Advertise, Some(DiscContents::Pub(publisher)))
}

pub(crate) fn text(frame: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(frame).map_err(|_| Error::InvalidFrame)
}
