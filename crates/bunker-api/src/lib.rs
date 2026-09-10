//! The LAN BUNKER backend. No module below `routers` uses HTTP, and no module
//! above `storage` uses SQL. `CLAUDE.md` gives the layer diagram and the rules.

pub mod config;
pub mod internal;
pub mod routers;
pub mod server;
pub mod services;
pub mod storage;
