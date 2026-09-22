//! Applies a [`Task`]'s rubric to a [`RunResult`], producing a scored
//! report. Every objective [`Check`] here is a pure function of already-
//! captured artifacts — no network, no re-running the backend.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::runner::RunResult;
use crate::task::{Check, Criterion, Task};

/// One criterion's outcome. `passed: None` means "not automated" — a
/// [`Check::Subjective`] criterion — never "unknown due to an error" (an
/// error while checking is a `Some(false)` with the reason in `detail`).
#[derive(Debug, Clone, Serialize)]
pub struct CriterionResult {
    pub id: String,
    pub description: String,
    pub objective: bool,
    pub passed: Option<bool>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoreReport {
    pub task_id: String,
    pub results: Vec<CriterionResult>,
}

impl ScoreReport {
    /// `(passed, total)` counted over objective criteria only — a
    /// subjective criterion has no pass/fail to count yet.
    pub fn objective_pass_count(&self) -> (usize, usize) {
        let objective: Vec<&CriterionResult> =
            self.results.iter().filter(|r| r.objective).collect();
        let passed = objective.iter().filter(|r| r.passed == Some(true)).count();
        (passed, objective.len())
    }
}

pub fn score(task: &Task, run: &RunResult) -> ScoreReport {
    let results = task.rubric.iter().map(|c| score_one(c, run)).collect();
    ScoreReport {
        task_id: task.id.clone(),
        results,
    }
}

fn score_one(criterion: &Criterion, run: &RunResult) -> CriterionResult {
    let (objective, passed, detail) = match &criterion.check {
        Check::StagesPass => score_stages_pass(run),
        Check::MinDecisionConfidence { threshold } => score_min_confidence(run, *threshold),
        Check::DrcClean => score_drc_clean(run),
        Check::Subjective => (false, None, "needs human review".to_string()),
    };
    CriterionResult {
        id: criterion.id.clone(),
        description: criterion.description.clone(),
        objective,
        passed,
        detail,
    }
}

fn score_stages_pass(run: &RunResult) -> (bool, Option<bool>, String) {
    if run.stages.is_empty() {
        return (true, Some(false), "no stages ran".to_string());
    }
    let failed: Vec<&str> = run
        .stages
        .iter()
        .filter(|s| !s.passed())
        .map(|s| s.stage.as_str())
        .collect();
    if failed.is_empty() {
        (
            true,
            Some(true),
            format!("all {} stage(s) exited 0", run.stages.len()),
        )
    } else {
        (true, Some(false), format!("failed: {}", failed.join(", ")))
    }
}

fn score_min_confidence(run: &RunResult, threshold: f64) -> (bool, Option<bool>, String) {
    let Some(path) = &run.decision_trace else {
        return (true, Some(false), "no decision trace captured".to_string());
    };
    match read_trace_confidences(path) {
        Ok(confidences) if confidences.is_empty() => {
            (true, Some(false), "decision trace was empty".to_string())
        }
        Ok(confidences) => {
            let below: Vec<String> = confidences
                .iter()
                .filter(|(_, c)| *c < threshold)
                .map(|(k, c)| format!("{k}={c:.2}"))
                .collect();
            if below.is_empty() {
                (
                    true,
                    Some(true),
                    format!("all {} decision(s) >= {threshold:.2}", confidences.len()),
                )
            } else {
                (
                    true,
                    Some(false),
                    format!("below {threshold:.2}: {}", below.join(", ")),
                )
            }
        }
        Err(e) => (true, Some(false), format!("could not read trace: {e}")),
    }
}

fn score_drc_clean(run: &RunResult) -> (bool, Option<bool>, String) {
    match run.stages.iter().find(|s| s.stage == "drc") {
        None => (true, Some(false), "drc stage did not run".to_string()),
        Some(s) => {
            let first_line = s.stdout.lines().next().unwrap_or("").to_string();
            let clean = s.passed() && s.stdout.contains(": 0 error(s)");
            (true, Some(clean), first_line)
        }
    }
}

fn read_trace_confidences(path: &Path) -> anyhow::Result<Vec<(String, f64)>> {
    let text = std::fs::read_to_string(path)?;
    let records: Vec<Value> = serde_json::from_str(&text)?;
    Ok(records
        .into_iter()
        .filter_map(|r| {
            let key = r.get("key")?.as_str()?.to_string();
            let confidence = r.get("confidence")?.as_f64()?;
            Some((key, confidence))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::StageRun;

    fn stage(name: &str, ok: bool) -> StageRun {
        StageRun {
            stage: name.to_string(),
            command: name.to_string(),
            exit_code: Some(if ok { 0 } else { 1 }),
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    #[test]
    fn stages_pass_fails_loud_on_any_failed_stage() {
        let run = RunResult {
            stages: vec![stage("spec", true), stage("schematic", false)],
            ..Default::default()
        };
        let (objective, passed, detail) = score_stages_pass(&run);
        assert!(objective);
        assert_eq!(passed, Some(false));
        assert!(detail.contains("schematic"));
    }

    #[test]
    fn stages_pass_with_no_stages_is_a_failure_not_a_vacuous_pass() {
        let run = RunResult::default();
        let (_, passed, _) = score_stages_pass(&run);
        assert_eq!(passed, Some(false));
    }

    #[test]
    fn min_confidence_reads_a_real_trace_file() {
        let dir = std::env::temp_dir().join(format!("pcbbench-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let trace_path = dir.join("trace.json");
        std::fs::write(
            &trace_path,
            r#"[{"key":"a","type":"choice","chosen":"x","confidence":0.9,"timestamp_unix":0},
                {"key":"b","type":"noul","chosen":"0.400","confidence":0.4,"timestamp_unix":0}]"#,
        )
        .unwrap();
        let run = RunResult {
            decision_trace: Some(trace_path),
            ..Default::default()
        };
        let (_, passed, detail) = score_min_confidence(&run, 0.7);
        assert_eq!(passed, Some(false));
        assert!(detail.contains("b=0.40"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn min_confidence_missing_trace_fails_not_panics() {
        let run = RunResult::default();
        let (_, passed, detail) = score_min_confidence(&run, 0.7);
        assert_eq!(passed, Some(false));
        assert!(detail.contains("no decision trace"));
    }

    #[test]
    fn subjective_criterion_has_no_pass_fail() {
        let task = Task {
            id: "t".into(),
            family: "fuzz-pedal".into(),
            brief: "b".into(),
            rubric: vec![Criterion {
                id: "vibe".into(),
                description: "d".into(),
                check: Check::Subjective,
            }],
        };
        let report = score(&task, &RunResult::default());
        assert_eq!(report.results[0].passed, None);
        assert!(!report.results[0].objective);
        assert_eq!(report.objective_pass_count(), (0, 0));
    }
}
