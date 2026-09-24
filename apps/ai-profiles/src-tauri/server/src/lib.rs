//! ai-profiles-server: runs on a Linux machine as the user, and lets the
//! ai-profiles app list that machine's Claude accounts and sessions (and, in
//! later versions, start and resume them in tmux with Remote Control).
//!
//! It speaks HTTPS with a self-signed certificate that clients pin, and each
//! client proves itself with a token it got by pairing. See the README in
//! this folder for setting it up.

pub mod accounts;
pub mod agents;
pub mod certs;
pub mod config;
pub mod dirs;
pub mod error;
pub mod hostinfo;
pub mod launch;
pub mod limits;
pub mod login;
pub mod moves;
pub mod pairing_code;
pub mod procs;
pub mod rename;
pub mod revive;
pub mod routes;
pub mod serve;
pub mod service;
pub mod sessions;
pub mod setup;
pub mod store;
pub mod tmux;
pub mod tunnels;
