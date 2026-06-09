//! Shared test fixtures.
//!
//! `app_data_dir()` resolves to a single filesystem path that every module
//! shares. The `app_state` and `profiles` tests each `remove_dir_all` it
//! before/after their assertions, so they must serialize against each
//! other — a per-module mutex would still let cross-module tests race.
//! This module hosts the global mutex they all lock.

#![cfg(test)]

use std::sync::Mutex;

pub(crate) static APP_DIR_TEST_LOCK: Mutex<()> = Mutex::new(());
