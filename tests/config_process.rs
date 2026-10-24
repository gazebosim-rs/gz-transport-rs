// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use gz_transport::{Config, Error};
use std::process::Command;
#[test]
fn config_environment_child() {
    match std::env::var("GZ_CONFIG_FIXTURE").as_deref() {
        Ok("valid") => {
            let config = Config::from_env().unwrap();
            assert_eq!(config.partition, "from-child");
            assert_eq!((config.message_port, config.service_port), (11517, 11518));
            assert_eq!(
                (config.send_high_watermark, config.receive_high_watermark),
                (9, 8)
            );
        }
        Ok("non-unicode") => assert!(matches!(Config::from_env(), Err(Error::InvalidConfig))),
        _ => {}
    }
}
#[test]
fn explicit_process_environment_capture() {
    let mut child = Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", "config_environment_child"]);
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GZ_"))
    {
        child.env_remove(key);
    }
    child
        .env("GZ_CONFIG_FIXTURE", "valid")
        .env("GZ_PARTITION", "from-child")
        .env("GZ_DISCOVERY_MSG_PORT", "11517")
        .env("GZ_DISCOVERY_SRV_PORT", "11518")
        .env("GZ_TRANSPORT_SNDHWM", "9")
        .env("GZ_TRANSPORT_RCVHWM", "8");
    assert!(child.status().unwrap().success());
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        child
            .env("GZ_CONFIG_FIXTURE", "non-unicode")
            .env("GZ_PARTITION", std::ffi::OsString::from_vec(vec![255]));
        assert!(child.status().unwrap().success());
    }
}
