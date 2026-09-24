//! What the ai-profiles app and ai-profiles-server share: reading Claude
//! Code's files (transcripts, the account a config directory is signed in
//! under, the registry of running sessions), moving and archiving sessions and
//! merging project memory the way claudemulti does, running a program with a
//! time limit, the API between the two, the
//! pairing code, and — with the `tls` feature — trusting a server by its
//! certificate's fingerprint. No platform code, so the server can build it
//! for Linux.

pub mod account;
pub mod api;
pub mod child;
pub mod memory;
pub mod pairing;
pub mod registry;
pub mod session_move;
#[cfg(feature = "tls")]
pub mod tls;
pub mod transcript;
