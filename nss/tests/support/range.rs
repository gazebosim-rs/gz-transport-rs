// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[test]
fn every_range_is_checked_before_owned_buffer_access() {
    let buffer = [0u8; 8];
    let base = buffer.as_ptr().addr();
    assert_eq!(within(&buffer, base, 8), Ok(0..8));
    assert_eq!(within(&buffer, base + 4, 4), Ok(4..8));
    for (pointer, length) in [
        (base - 1, 4),
        (base + 5, 4),
        (usize::MAX, 8),
        (base, usize::MAX),
        (base + 1, usize::MAX),
    ] {
        assert_eq!(
            within(&buffer, pointer, length),
            Err(Error::MalformedResult)
        );
    }
}
