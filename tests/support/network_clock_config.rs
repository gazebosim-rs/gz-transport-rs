// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_transport::Config;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddrV4};

pub fn config() -> (Config, [Socket; 2]) {
    let reserve = || {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        socket.set_reuse_address(true).unwrap();
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
            .unwrap();
        #[cfg(target_os = "linux")]
        socket.set_multicast_all_v4(false).unwrap();
        socket
    };
    let reservations = [reserve(), reserve()];
    let ports = reservations.each_ref().map(|socket| {
        socket
            .local_addr()
            .unwrap()
            .as_socket_ipv4()
            .unwrap()
            .port()
    });
    assert_ne!(ports[0], ports[1]);
    (
        Config {
            partition: uuid::Uuid::new_v4().to_string(),
            message_port: ports[0],
            service_port: ports[1],
            ..Default::default()
        },
        reservations,
    )
}
