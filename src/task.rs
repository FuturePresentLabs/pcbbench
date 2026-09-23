//! Task definition: a design brief plus a rubric to score the result
//! against. Deliberately EDA-tool-agnostic — nothing here knows about
//! legion-of-bom specifically; that's the runner's job ([`crate::runner`]).
//!
//! The container ([`Task`]/[`Criterion`]) is [`eval::Task`]/[`eval::Criterion`]
//! — shared scaffolding every sibling harness in this ecosystem uses.
//! [`Check`] is the one part that's actually EDA-specific.

use serde::{Deserialize, Serialize};

/// One eval task: what to design, and what "good" means.
pub type Task = eval::Task<Check>;

/// One rubric line item.
pub type Criterion = eval::Criterion<Check>;

/// What a criterion actually checks. A real enum (not a bool flag) so a
/// check carries the data it needs — e.g. a confidence threshold — instead
/// of that living somewhere else the two can drift apart.
///
/// `objective`/`subjective` (DESIGN.md-style framing carried over from
/// legion-of-bom's own eval-0 planning) is a property of *which variant*
/// this is, not a separate field: [`Check::Subjective`] is the only kind
/// that isn't automatically scored, so there's nothing to keep in sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    /// Every pipeline stage that actually ran must have exited 0.
    StagesPass,
    /// Every decision in the run's trace must clear this confidence
    /// threshold (0.0-1.0).
    MinDecisionConfidence { threshold: f64 },
    /// DRC must report zero errors.
    DrcClean,
    /// A sine sweep driven into the rendered circuit's input net must show
    /// real clipping/gain-compression behavior — not a schematic-reading
    /// guess, an actual measured SPICE claim (legion-of-bom's
    /// `scope_probe`/`simulate_tran_drive`, live-validated against real
    /// fuzz circuits: asymmetric BJT clipping, onset ~100-300mV,
    /// saturating toward 80-96% flat-top by 2V).
    ///
    /// `input_net_hint`, `freq_hz`, `min_amplitude_v`, `max_amplitude_v`,
    /// and `steps` are all optional — `lob scope-probe` has sensible
    /// built-in defaults for the sweep itself (net inference, 200Hz,
    /// 0.005V-1V, 8 log-spaced steps); a task only overrides what it
    /// actually needs to. `min_flat_top_at_max` has no such default (there
    /// is no universally sane "clips enough" bar) and is required. The
    /// runner only invokes `scope-probe` when a task's rubric actually
    /// declares this check, unlike `DrcClean`/`StagesPass` which always
    /// run.
    SpiceClips {
        input_net_hint: Option<String>,
        freq_hz: Option<f64>,
        min_amplitude_v: Option<f64>,
        max_amplitude_v: Option<f64>,
        steps: Option<u32>,
        min_flat_top_at_max: f64,
    },
    /// Not automated — a human fills this in. Named explicitly (not just
    /// "no check implemented yet") so a task file is honest about what it
    /// can't verify itself, and so "later every eval needs to be objective"
    /// has something concrete to point at and shrink over time.
    Subjective,
}

#[cfg(test)]
mod tests {
    use super::*;
    use eval::testing::assert_every_task_sound;
    use std::path::Path;

    #[test]
    fn task_round_trips_through_toml() {
        let task = Task {
            id: "fuzz-pedal-v1".into(),
            family: "fuzz-pedal".into(),
            brief: "vintage silicon fuzz, 9V, true bypass".into(),
            input: None,
            rubric: vec![
                Criterion {
                    id: "stages".into(),
                    description: "every stage that ran, passed".into(),
                    check: Check::StagesPass,
                },
                Criterion {
                    id: "confidence".into(),
                    description: "decisions are confident".into(),
                    check: Check::MinDecisionConfidence { threshold: 0.7 },
                },
                Criterion {
                    id: "drc".into(),
                    description: "board is DRC-clean".into(),
                    check: Check::DrcClean,
                },
                Criterion {
                    id: "vibe".into(),
                    description: "reads as a designed product, not generated".into(),
                    check: Check::Subjective,
                },
            ],
        };
        let text = toml::to_string_pretty(&task).unwrap();
        let back: Task = toml::from_str(&text).unwrap();
        assert_eq!(back.rubric.len(), 4);
        assert!(
            matches!(back.rubric[1].check, Check::MinDecisionConfidence { threshold } if threshold == 0.7)
        );
        assert!(matches!(back.rubric[3].check, Check::Subjective));
    }

    #[test]
    fn spice_clips_round_trips_with_an_optional_net_hint() {
        let check = Check::SpiceClips {
            input_net_hint: Some("IN".into()),
            freq_hz: Some(1000.0),
            min_amplitude_v: Some(0.005),
            max_amplitude_v: Some(1.0),
            steps: Some(8),
            min_flat_top_at_max: 0.6,
        };
        let text = toml::to_string_pretty(&check).unwrap();
        let back: Check = toml::from_str(&text).unwrap();
        assert!(matches!(
            back,
            Check::SpiceClips { input_net_hint: Some(ref h), min_flat_top_at_max, .. }
                if h == "IN" && min_flat_top_at_max == 0.6
        ));
    }

    #[test]
    fn spice_clips_round_trips_with_every_sweep_param_left_to_lobs_own_defaults() {
        let check = Check::SpiceClips {
            input_net_hint: None,
            freq_hz: None,
            min_amplitude_v: None,
            max_amplitude_v: None,
            steps: None,
            min_flat_top_at_max: 0.6,
        };
        let text = toml::to_string_pretty(&check).unwrap();
        let back: Check = toml::from_str(&text).unwrap();
        assert!(matches!(
            back,
            Check::SpiceClips { input_net_hint: None, freq_hz: None, .. }
        ));
    }

    /// Every shipped task file is part of the harness's contract, not sample
    /// data — see [`eval::testing::assert_every_task_sound`]'s docs. Parses
    /// every real file under `tasks/` rather than naming one, so a new task
    /// file is covered the moment it's added.
    #[test]
    fn every_shipped_task_parses_and_has_a_sound_rubric() {
        let tasks_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tasks"));
        assert_every_task_sound::<Check>(
            tasks_dir,
            &[
                ("a stages_pass criterion", |r| {
                    r.iter().any(|c| matches!(c.check, Check::StagesPass))
                }),
                ("a drc_clean criterion", |r| {
                    r.iter().any(|c| matches!(c.check, Check::DrcClean))
                }),
                ("a min_decision_confidence criterion", |r| {
                    r.iter()
                        .any(|c| matches!(c.check, Check::MinDecisionConfidence { .. }))
                }),
            ],
        );
    }
}
