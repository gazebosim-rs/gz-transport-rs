# Gazebo Transport in Rust

Independent interoperable Gazebo discovery, pub/sub, services, logging, and transport tooling.

This public repository is a planning shell. No Rust implementation, passing
migration test suite, or coverage result is claimed yet.

The architecture and dependency-ordered work packets live in
[gz-sim-rs](https://github.com/gazebosim-rs/gz-sim-rs/blob/main/docs/PLAN.md). Read this repository's
[implementation contract](https://github.com/gazebosim-rs/gz-sim-rs/blob/main/docs/EXTERNAL_REPOS.md#gz-transport-rs),
[test and coverage policy](https://github.com/gazebosim-rs/gz-sim-rs/blob/main/docs/TESTING.md), and
[decision register](https://github.com/gazebosim-rs/gz-sim-rs/blob/main/docs/DECISIONS.md) before implementation.

The owner selected the upstream Gazebo main branches, behavior/file-format/protocol
compatibility with new Rust APIs, faithful ports of existing tests verified against
C++ reference runs, and Linux first. Handwritten production Rust must have coverage
strictly above 95%, enforced through pre-commit and CI as implementation begins.

This repository remains independent of the gz-sim-rs Cargo workspace. Publish a
versioned Rust API and test it with the pinned cross-repository integration set.

License: Apache-2.0. Preserve upstream notices and fixture provenance when porting.
