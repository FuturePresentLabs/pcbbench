//! Task definition: a design brief plus a rubric to score the result
//! against. Deliberately EDA-tool-agnostic — nothing here knows about
//! legion-of-bom specifically; that's the runner's job ([`crate::runner`]).

use serde::{Deserialize, Serialize};

/// One eval task: what to design, and what "good" means.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task id, e.g. `"fuzz-pedal-v1"`.
    pub id: String,
    /// Circuit family the backend's spec-generation step should target
    /// (a backend-specific string — legion-of-bom's `lob spec` calls this a
    /// "family", e.g. `"fuzz-pedal"`).
    pub family: String,
    /// Free-text design brief, carried through to the backend's decision
    /// layer as context. Never parsed for control flow here or in the
    /// backend — matching legion-of-bom's own "the decisions, not the
    /// brief, choose the circuit" stance.
    pub brief: String,
    pub rubric: Vec<Criterion>,
}

/// One rubric line item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    pub id: String,
    pub description: String,
    #[serde(flatten)]
    pub check: Check,
}

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
    /// data: if one stops parsing, or quietly loses a criterion, every score
    /// it ever produces is wrong. Parses every real file under `tasks/`
    /// rather than naming one, so a new task file is covered the moment it's
    /// added -- no test to remember to write alongside it. (Mirrors
    /// cadbench's `every_shipped_task_parses_and_has_a_sound_rubric`, kept
    /// deliberately identical in shape.)
    #[test]
    fn every_shipped_task_parses_and_has_a_sound_rubric() {
        let tasks_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tasks");
        let mut checked = 0;
        for entry in std::fs::read_dir(tasks_dir).expect("tasks/ is readable") {
            let path = entry.expect("dir entry readable").path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()));
            let task: Task = toml::from_str(&text)
                .unwrap_or_else(|e| panic!("{} parses: {e}", path.display()));

            assert!(!task.id.trim().is_empty(), "{}: empty id", path.display());
            assert!(
                !task.family.trim().is_empty(),
                "{}: empty family",
                path.display()
            );
            assert!(
                !task.brief.trim().is_empty(),
                "{}: empty brief",
                path.display()
            );

            assert!(
                task.rubric.iter().any(|c| matches!(c.check, Check::StagesPass)),
                "{}: missing a stages_pass criterion",
                path.display()
            );
            assert!(
                task.rubric.iter().any(|c| matches!(c.check, Check::DrcClean)),
                "{}: missing a drc_clean criterion",
                path.display()
            );
            assert!(
                task.rubric
                    .iter()
                    .any(|c| matches!(c.check, Check::MinDecisionConfidence { .. })),
                "{}: missing a min_decision_confidence criterion",
                path.display()
            );

            let mut ids: Vec<&str> = task.rubric.iter().map(|c| c.id.as_str()).collect();
            ids.sort_unstable();
            let before = ids.len();
            ids.dedup();
            assert_eq!(ids.len(), before, "{}: duplicate rubric ids", path.display());

            checked += 1;
        }
        assert!(checked > 0, "tasks/ has no .toml files to check");
    }
}
