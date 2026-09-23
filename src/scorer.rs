//! Applies a [`Task`]'s rubric to a [`RunResult`], producing a scored
//! report. Every objective [`Check`] here is a pure function of already-
//! captured artifacts — no network, no re-running the backend.

pub use eval::{CriterionResult, ScoreReport, Verdict};

use std::path::Path;

use serde_json::Value;

use crate::runner::RunResult;
use crate::task::{Check, Criterion, Task};

/// Scores `run` against `task`'s rubric.
#[must_use]
pub fn score(task: &Task, run: &RunResult) -> ScoreReport {
    let results = task.rubric.iter().map(|c| score_one(c, run)).collect();
    ScoreReport {
        task_id: task.id.clone(),
        backend: run.backend.clone(),
        results,
    }
}

fn score_one(criterion: &Criterion, run: &RunResult) -> CriterionResult {
    let (verdict, detail) = match &criterion.check {
        Check::StagesPass => score_stages_pass(run),
        Check::MinDecisionConfidence { threshold } => score_min_confidence(run, *threshold),
        Check::DrcClean => score_drc_clean(run),
        Check::SpiceClips {
            min_flat_top_at_max,
            ..
        } => score_spice_clips(run, *min_flat_top_at_max),
        Check::Subjective => (Verdict::NeedsHuman, "needs human review".to_string()),
    };
    CriterionResult {
        id: criterion.id.clone(),
        description: criterion.description.clone(),
        verdict,
        detail,
    }
}

fn score_stages_pass(run: &RunResult) -> (Verdict, String) {
    if run.stages.is_empty() {
        return (Verdict::Fail, "no stages ran".to_string());
    }
    let failed: Vec<&str> = run
        .stages
        .iter()
        .filter(|s| !s.passed())
        .map(|s| s.stage.as_str())
        .collect();
    if failed.is_empty() {
        (
            Verdict::Pass,
            format!("all {} stage(s) exited 0", run.stages.len()),
        )
    } else {
        (Verdict::Fail, format!("failed: {}", failed.join(", ")))
    }
}

fn score_min_confidence(run: &RunResult, threshold: f64) -> (Verdict, String) {
    let Some(path) = &run.decision_trace else {
        return (Verdict::Fail, "no decision trace captured".to_string());
    };
    match read_trace_confidences(path) {
        Ok(confidences) if confidences.is_empty() => {
            (Verdict::Fail, "decision trace was empty".to_string())
        }
        Ok(confidences) => {
            let below: Vec<String> = confidences
                .iter()
                .filter(|(_, c)| *c < threshold)
                .map(|(k, c)| format!("{k}={c:.2}"))
                .collect();
            if below.is_empty() {
                (
                    Verdict::Pass,
                    format!("all {} decision(s) >= {threshold:.2}", confidences.len()),
                )
            } else {
                (
                    Verdict::Fail,
                    format!("below {threshold:.2}: {}", below.join(", ")),
                )
            }
        }
        Err(e) => (Verdict::Fail, format!("could not read trace: {e}")),
    }
}

fn score_drc_clean(run: &RunResult) -> (Verdict, String) {
    match run.stages.iter().find(|s| s.stage == "drc") {
        None => (Verdict::Fail, "drc stage did not run".to_string()),
        Some(s) => {
            let first_line = s.stdout.lines().next().unwrap_or("").to_string();
            let clean = s.passed() && s.stdout.contains(": 0 error(s)");
            (
                if clean { Verdict::Pass } else { Verdict::Fail },
                first_line,
            )
        }
    }
}

fn score_spice_clips(run: &RunResult, min_flat_top_at_max: f64) -> (Verdict, String) {
    let Some(path) = &run.scope_probe_json else {
        return (
            Verdict::Fail,
            "scope-probe did not produce a report".to_string(),
        );
    };
    match read_flat_top(path) {
        Ok(flat_top) if flat_top >= min_flat_top_at_max => (
            Verdict::Pass,
            format!("flat-top {flat_top:.2} >= {min_flat_top_at_max:.2} at max amplitude"),
        ),
        Ok(flat_top) => (
            Verdict::Fail,
            format!("flat-top {flat_top:.2} < {min_flat_top_at_max:.2} at max amplitude"),
        ),
        Err(e) => (
            Verdict::Fail,
            format!("could not read scope-probe report: {e}"),
        ),
    }
}

fn read_flat_top(path: &Path) -> anyhow::Result<f64> {
    let text = std::fs::read_to_string(path)?;
    let report: Value = serde_json::from_str(&text)?;
    report
        .get("flat_top_at_max")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow::anyhow!("report missing flat_top_at_max"))
}

fn read_trace_confidences(path: &Path) -> anyhow::Result<Vec<(String, f64)>> {
    let text = std::fs::read_to_string(path)?;
    let records: Vec<Value> = serde_json::from_str(&text)?;
    Ok(records
        .into_iter()
        .filter_map(|r| {
            let key = r.get("key")?.as_str()?.to_string();
            let confidence = r.get("confidence")?.as_f64()?;
            // ooda records a yes/no (noul) answer's probability as its
            // confidence; a firm NO at p = 0.02 is 0.98 sure, not 0.02.
            let decisive = match r.get("kind").and_then(Value::as_str) {
                Some("noul") => confidence.max(1.0 - confidence),
                _ => confidence,
            };
            Some((key, decisive))
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
        let (verdict, detail) = score_stages_pass(&run);
        assert_eq!(verdict, Verdict::Fail);
        assert!(detail.contains("schematic"));
    }

    #[test]
    fn stages_pass_with_no_stages_is_a_failure_not_a_vacuous_pass() {
        let run = RunResult::default();
        let (verdict, _) = score_stages_pass(&run);
        assert_eq!(verdict, Verdict::Fail);
    }

    #[test]
    fn min_confidence_reads_a_real_trace_file() {
        let dir = std::env::temp_dir().join(format!("pcbbench-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let trace_path = dir.join("trace.json");
        std::fs::write(
            &trace_path,
            r#"[{"key":"a","kind":"choice","chosen":"x","confidence":0.9,"timestamp_unix":0},
                {"key":"b","kind":"noul","chosen":"0.400","confidence":0.4,"timestamp_unix":0},
                {"key":"c","kind":"noul","chosen":"0.020","confidence":0.02,"timestamp_unix":0}]"#,
        )
        .unwrap();
        let run = RunResult {
            decision_trace: Some(trace_path),
            ..Default::default()
        };
        let (verdict, detail) = score_min_confidence(&run, 0.7);
        assert_eq!(verdict, Verdict::Fail);
        // A hedged yes/no (p = 0.4) is 0.6 sure; a firm no (p = 0.02) is 0.98.
        assert!(detail.contains("b=0.60"), "{detail}");
        assert!(!detail.contains("c="), "{detail}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn min_confidence_missing_trace_fails_not_panics() {
        let run = RunResult::default();
        let (verdict, detail) = score_min_confidence(&run, 0.7);
        assert_eq!(verdict, Verdict::Fail);
        assert!(detail.contains("no decision trace"));
    }

    #[test]
    fn spice_clips_passes_when_flat_top_clears_the_threshold() {
        let dir = std::env::temp_dir().join(format!("pcbbench-test-sc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let report_path = dir.join("scope-probe.json");
        std::fs::write(&report_path, r#"{"flat_top_at_max":0.91}"#).unwrap();
        let run = RunResult {
            scope_probe_json: Some(report_path),
            ..Default::default()
        };
        let (verdict, detail) = score_spice_clips(&run, 0.8);
        assert_eq!(verdict, Verdict::Pass);
        assert!(detail.contains("0.91"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn spice_clips_fails_when_flat_top_misses_the_threshold() {
        let dir = std::env::temp_dir().join(format!("pcbbench-test-sc2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let report_path = dir.join("scope-probe.json");
        std::fs::write(&report_path, r#"{"flat_top_at_max":0.42}"#).unwrap();
        let run = RunResult {
            scope_probe_json: Some(report_path),
            ..Default::default()
        };
        let (verdict, _) = score_spice_clips(&run, 0.8);
        assert_eq!(verdict, Verdict::Fail);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn spice_clips_missing_report_fails_not_panics() {
        let (verdict, detail) = score_spice_clips(&RunResult::default(), 0.8);
        assert_eq!(verdict, Verdict::Fail);
        assert!(detail.contains("did not produce a report"));
    }

    #[test]
    fn subjective_criterion_has_no_pass_fail() {
        let task = Task {
            id: "t".into(),
            family: "fuzz-pedal".into(),
            brief: "b".into(),
            input: None,
            rubric: vec![Criterion {
                id: "vibe".into(),
                description: "d".into(),
                check: Check::Subjective,
            }],
        };
        let report = score(&task, &RunResult::default());
        assert_eq!(report.results[0].verdict, Verdict::NeedsHuman);
        assert!(report.all_automated_pass());
        assert_eq!(report.needs_human(), vec!["vibe"]);
    }

    #[test]
    fn a_real_fail_shows_up_in_all_automated_pass() {
        let task = Task {
            id: "t".into(),
            family: "fuzz-pedal".into(),
            brief: "b".into(),
            input: None,
            rubric: vec![Criterion {
                id: "stages".into(),
                description: "d".into(),
                check: Check::StagesPass,
            }],
        };
        let report = score(&task, &RunResult::default());
        assert!(!report.all_automated_pass());
    }
}
