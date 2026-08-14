//! Squash benchmark harness (docs/05 §6, docs/01 §3.8).
//!
//! End-to-end process/job timing — not micro-benchmarks (no criterion):
//! wall-clock around real `Engine` jobs and real competitor CLI processes on
//! the standard corpus (`benches/corpus/`), recording ratio + speed so the
//! docs/01 §4 claims (match/beat 7-Zip's default ratio, zstd speed wins) are
//! measurable and CI-gateable.
//!
//! Subcommands (see `cli`): `corpus generate`, `run` (squash only),
//! `compare` (squash + detected competitors), `report` (markdown), `check`
//! (regression gate vs `benches/baseline.json`).

pub mod check;
pub mod cli;
pub mod competitors;
pub mod corpus;
pub mod levels;
pub mod machine;
pub mod model;
pub mod prng;
pub mod report;
pub mod runner;
