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
