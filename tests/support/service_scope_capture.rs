// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_msgs::{
    discovery::{
        Discovery,
        discovery::{DiscContents, Type, publisher::PubType},
    },
    prost::Message,
};
use socket2::{Domain, Protocol, Socket, Type as SocketType};
use std::{
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    time::{Duration, Instant},
};
pub fn listener() -> UdpSocket {
    let socket = Socket::new(Domain::IPV4, SocketType::DGRAM, Some(Protocol::UDP)).unwrap();
    socket.set_reuse_address(true).unwrap();
    socket
        .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
        .unwrap();
    socket
        .join_multicast_v4(&Ipv4Addr::new(239, 255, 0, 7), &Ipv4Addr::LOCALHOST)
        .unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(10)))
        .unwrap();
    socket.into()
}
pub fn receive(socket: &UdpSocket) -> Option<Discovery> {
    let mut bytes = [0; 65507];
    match socket.recv_from(&mut bytes) {
        Ok((len, _)) => {
            assert!(len >= 2);
            assert_eq!(u16::from_ne_bytes([bytes[0], bytes[1]]) as usize, len - 2);
            let msg = Discovery::decode(&bytes[2..len]).unwrap();
            assert_eq!(msg.version, 10);
            Some(msg)
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            None
        }
        Err(error) => panic!("native capture failed: {error}"),
    }
}
pub fn collect(socket: &UdpSocket, uuid: &str, kind: Type, time: Duration) -> Vec<Discovery> {
    let deadline = Instant::now() + time;
    let mut found = vec![];
    while Instant::now() < deadline {
        if let Some(msg) = receive(socket)
            && msg.process_uuid == uuid
            && msg.r#type == kind as i32
        {
            found.push(msg);
        }
    }
    found
}
pub fn check_record(message: &Discovery, fields: &[String]) {
    let Some(DiscContents::Pub(p)) = &message.disc_contents else {
        panic!("no publisher record")
    };
    let Some(PubType::SrvPub(s)) = &p.pub_type else {
        panic!("not service metadata")
    };
    assert_eq!(
        [
            p.topic.clone(),
            p.address.clone(),
            p.process_uuid.clone(),
            p.node_uuid.clone(),
            p.scope.to_string(),
            s.socket_id.clone(),
            s.request_type.clone(),
            s.response_type.clone()
        ],
        fields
    );
}
