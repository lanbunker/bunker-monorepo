//! The cabinet software as a library. The binary in `bunker-cabd` adds the SDL2
//! screen and nothing else.
//!
//! `cabinet`, `view` and `hiscore` are pure: no I/O, no threads, no clock of
//! their own. `config`, `launcher` and `conductor` do the I/O around them.
//! `DESIGN.md` in `crates/bunker-cabd` gives the reasons.

pub mod cabinet;
pub mod conductor;
pub mod config;
pub mod hiscore;
pub mod launcher;
pub mod view;

pub use conductor::{ConductorGone, ConductorPanicked, Handle, StartError, error_chain, start};
