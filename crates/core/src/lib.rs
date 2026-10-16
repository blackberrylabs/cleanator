//! Cleanator core. See `docs/rules.md` and `BRIEF.md`.
//!
//! Three invariants, from the project brief:
//! - **R1** this crate never deletes anything: it scans, classifies and writes a script;
//! - **R2** what no rule recognises is `never`;
//! - **R10 bis** only metadata is read (names, sizes, dates, inodes), never file contents.

pub mod classify;
pub mod exec;
pub mod inuse;
pub mod paths;
pub mod probes;
pub mod rules;
pub mod scan;
pub mod script;
pub mod size;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
