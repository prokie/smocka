//! Integration tests, compiled as a single crate so Bevy and lightyear are only linked once.
//! Add new suites as modules here rather than as new files directly under `tests/`.

mod common;
mod lobby;
mod network;
mod pong;
