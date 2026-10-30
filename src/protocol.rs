// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::Error;
use gz_msgs::{
    WireMessage,
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

/// Rewrite only Flags in an already validated discovery frame. Merging its
/// occurrences lets a full-size relayed packet stay within the native limit.
pub(crate) fn relay_frame(frame: &[u8], unicast: bool) -> Result<Vec<u8>, Error> {
    use gz_msgs::prost::encoding::{self, DecodeContext, WireType, bool as boolean, bytes};
    if frame.len() < 2 || frame.len() > MAX_DATAGRAM {
        return Err(Error::InvalidFrame);
    }
    let mut input = &frame[2..];
    let mut output = Vec::with_capacity((frame.len() + 4).min(MAX_DATAGRAM));
    output.extend_from_slice(&[0, 0]);
    let mut flags = Vec::new();
    while !input.is_empty() {
        let original = input;
        let (tag, wire) = encoding::decode_key(&mut input).map_err(|_| Error::InvalidFrame)?;
        if tag == 5 && wire == WireType::LengthDelimited {
            let length = encoding::decode_varint(&mut input).map_err(|_| Error::InvalidFrame)?;
            if length > input.len() as u64 {
                return Err(Error::InvalidFrame);
            }
            let length = length as usize;
            let mut fields = &input[..length];
            input = &input[length..];
            while !fields.is_empty() {
                let original = fields;
                let (tag, wire) =
                    encoding::decode_key(&mut fields).map_err(|_| Error::InvalidFrame)?;
                encoding::skip_field(wire, tag, &mut fields, DecodeContext::default())
                    .map_err(|_| Error::InvalidFrame)?;
                // A wrong-wire occurrence is unknown, not an edited bool. All
                // other flag bytes retain their order and exact representation.
                if wire != WireType::Varint || (tag != 1 && (unicast || tag != 2)) {
                    flags.extend_from_slice(&original[..original.len() - fields.len()]);
                }
            }
        } else {
            encoding::skip_field(wire, tag, &mut input, DecodeContext::default())
                .map_err(|_| Error::InvalidFrame)?;
            output.extend_from_slice(&original[..original.len() - input.len()]);
        }
    }
    // Reinjection removes every known relay bool, so its native default false
    // needs no extra encoded field. Unicast preserves the no_relay value.
    boolean::encode(if unicast { 1 } else { 2 }, &true, &mut flags);
    let size = output
        .len()
        .checked_add(bytes::encoded_len(5, &flags))
        .ok_or(Error::InvalidFrame)?;
    if size > MAX_DATAGRAM {
        return Err(Error::InvalidFrame);
    }
    bytes::encode(5, &flags, &mut output);
    output[..2].copy_from_slice(&((size - 2) as u16).to_ne_bytes());
    Ok(output)
}

pub(crate) fn decode(frame: &[u8]) -> Result<Discovery, Error> {
    decode_version(frame, VERSION)
}

pub(crate) fn decode_version(frame: &[u8], version: u32) -> Result<Discovery, Error> {
    if frame.len() < 2
        || frame.len() > MAX_DATAGRAM
        || u16::from_ne_bytes([frame[0], frame[1]]) as usize != frame.len() - 2
    {
        return Err(Error::InvalidFrame);
    }
    let msg = WireMessage::<Discovery>::decode_compatible(&frame[2..])
        .map_err(|_| Error::InvalidFrame)?
        .value()
        .clone();
    if msg.version != version {
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

#[cfg(test)]
#[path = "../tests/support/discovery_wire.rs"]
mod tests;
