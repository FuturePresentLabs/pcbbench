//! PCBBench — an eval harness for typed-decision-driven PCB design agents.
//!
//! Deliberately EDA-tool-agnostic: it drives a backend as a subprocess
//! ([`runner`]) and scores the artifacts that come back against a task's
//! rubric ([`scorer`]). Today's only backend is legion-of-bom's `lob` CLI,
//! but nothing outside `runner`'s concrete `Command` invocations knows
//! that — a different backend is a different set of subprocess calls, not a
//! different architecture.

pub mod runner;
pub mod scorer;
pub mod task;
