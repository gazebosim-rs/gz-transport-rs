// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
//! Owned Linux GNU first-A lookup for explicit configuration setup.
#![cfg(all(target_os = "linux", target_env = "gnu"))]
use std::{ffi::CStr, mem::size_of, net::Ipv4Addr, ops::Range, ptr};

// Linux GNU netdb.h. libc 0.2.191 has hostent but no _r declaration.
unsafe extern "C" {
    fn gethostbyname_r(
        name: *const libc::c_char,
        record: *mut libc::hostent,
        buffer: *mut libc::c_char,
        length: libc::size_t,
        result: *mut *mut libc::hostent,
        host_error: *mut libc::c_int,
    ) -> libc::c_int;
}

const INITIAL_BYTES: usize = 1024;
const MAX_BYTES: usize = 64 * 1024;
const MAX_NAME_BYTES: usize = 1023;
const NETDB_INTERNAL: libc::c_int = -1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NameTooLong,
    Allocation,
    BufferLimit,
    Lookup {
        code: libc::c_int,
        host_error: libc::c_int,
    },
    MalformedResult,
}

// Compare integer addresses only. No returned foreign pointer is dereferenced;
// after validation we access the owned scratch allocation through its pointer.
fn within(buffer: &[u8], address: usize, length: usize) -> Result<Range<usize>, Error> {
    let start = address
        .checked_sub(buffer.as_ptr().addr())
        .ok_or(Error::MalformedResult)?;
    let end = start
        .checked_add(length)
        .filter(|end| *end <= buffer.len())
        .ok_or(Error::MalformedResult)?;
    Ok(start..end)
}

fn owned_first(record: &libc::hostent, buffer: &[u8]) -> Result<Option<Ipv4Addr>, Error> {
    if record.h_addrtype != libc::AF_INET || record.h_length != 4 || record.h_addr_list.is_null() {
        return Err(Error::MalformedResult);
    }
    let range = within(
        buffer,
        record.h_addr_list.addr(),
        size_of::<*const libc::c_char>(),
    )?;
    // SAFETY: range is within our live, initialized scratch allocation and
    // spans a whole raw pointer. libc success stores the first C pointer here.
    // read_unaligned avoids assuming a char buffer's Rust alignment.
    let address = unsafe {
        buffer
            .as_ptr()
            .add(range.start)
            .cast::<*const libc::c_char>()
            .read_unaligned()
    };
    if address.is_null() {
        return Ok(None);
    }
    let range = within(buffer, address.addr(), 4)?;
    let bytes: [u8; 4] = buffer[range]
        .try_into()
        .map_err(|_| Error::MalformedResult)?;
    Ok(Some(Ipv4Addr::from(bytes)))
}

/// Copy only the first IPv4 A record. No later pointer entries are inspected;
/// no pointer, alias, hostname or static hostent data escapes.
/// Only GNU libc is qualified here; NSS remains synchronous and not time-bounded.
/// Scratch is bounded at 64 KiB (seven calls). More total NSS result data can
/// still require a larger buffer and is an explicit bounded-error difference.
pub fn lookup_first_v4(name: &CStr) -> Result<Option<Ipv4Addr>, Error> {
    if name.to_bytes().len() > MAX_NAME_BYTES {
        return Err(Error::NameTooLong);
    }
    let mut length = INITIAL_BYTES;
    loop {
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(length)
            .map_err(|_| Error::Allocation)?;
        buffer.resize(length, 0u8);
        let mut record = libc::hostent {
            h_name: ptr::null_mut(),
            h_aliases: ptr::null_mut(),
            h_addrtype: 0,
            h_length: 0,
            h_addr_list: ptr::null_mut(),
        };
        let mut result = ptr::null_mut();
        let mut host_error = 0;
        // SAFETY: name is NUL-terminated and live for the call; record and both
        // output scalars are distinct writable stack storage; buffer owns length
        // initialized writable bytes, is not aliased and cannot move during C.
        // glibc's _r contract uses caller storage, unlike static gethostbyname.
        let code = unsafe {
            gethostbyname_r(
                name.as_ptr(),
                &mut record,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
                &mut host_error,
            )
        };
        if code == libc::ERANGE && host_error == NETDB_INTERNAL {
            length = length
                .checked_mul(2)
                .filter(|next| *next <= MAX_BYTES)
                .ok_or(Error::BufferLimit)?;
            continue;
        }
        if code != 0 {
            return Err(Error::Lookup { code, host_error });
        }
        if result.is_null() {
            return Ok(None);
        }
        if result != ptr::addr_of_mut!(record) {
            return Err(Error::MalformedResult);
        }
        return owned_first(&record, &buffer);
    }
}

#[cfg(test)]
#[path = "../tests/support/range.rs"]
mod tests;
