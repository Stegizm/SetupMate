//! SetupMate core library.
//!
//! This crate holds the GUI-independent logic: detection, catalog loading,
//! planning, execution and self-update support.

pub mod catalog;
pub mod detection;
pub mod executor;
pub mod installed;
pub mod models;
pub mod package_managers;
pub mod planner;
pub mod reporting;
pub mod sha256;
pub mod sys;
pub mod updater;
