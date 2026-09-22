//! Drives a design-agent backend as a subprocess and captures its
//! artifacts. Today's only backend is legion-of-bom's `lob` CLI; nothing in
//! this module is legion-of-bom-specific beyond the concrete subcommand
//! names below — a different backend is a different set of `Command`
//! invocations, not a different architecture (this is the whole point of
//! going through a subprocess boundary rather than linking `lob` as a
//! library).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::task::Task;

/// One subprocess run's outcome — exit status + captured output, kept for
/// the scorer and for a human to inspect after the fact.
#[derive(Debug, Clone)]
pub struct StageRun {
    pub stage: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl StageRun {
    pub fn passed(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Every artifact + stage result produced by one eval run. Stages after the
/// first failure are simply absent (not run) rather than recorded as
/// skipped — [`StageRun`] only exists for a stage that was actually
/// attempted.
#[derive(Debug, Clone, Default)]
pub struct RunResult {
    pub stages: Vec<StageRun>,
    pub spec_json: Option<PathBuf>,
    pub schematic_py: Option<PathBuf>,
    pub decision_trace: Option<PathBuf>,
}

/// Drive `lob` through concept → spec → design for `task`, writing
/// artifacts under `work_dir`.
///
/// `existing_spec`, when given, skips the live decision-making step
/// (`lob spec`, which needs a reachable SystemOne/Jev-compatible endpoint)
/// and starts from an already-written spec JSON instead — for replaying or
/// testing the rest of the pipeline without a network call, the same
/// concept-vs-design split `lob schematic` itself is built around.
pub fn run(
    lob_bin: &Path,
    task: &Task,
    work_dir: &Path,
    existing_spec: Option<&Path>,
) -> Result<RunResult> {
    std::fs::create_dir_all(work_dir).context("creating work dir")?;
    let mut result = RunResult::default();

    let spec_json = work_dir.join("design.json");
    let trace_json = work_dir.join("design.trace.json");

    if let Some(existing) = existing_spec {
        std::fs::copy(existing, &spec_json)
            .with_context(|| format!("copying existing spec from {}", existing.display()))?;
    } else {
        let stage = run_stage(
            "spec",
            Command::new(lob_bin)
                .arg("spec")
                .arg(&task.family)
                .arg("--brief")
                .arg(&task.brief)
                .arg("--out")
                .arg(work_dir.join("design"))
                .arg("--trace")
                .arg(&trace_json),
        )?;
        let passed = stage.passed();
        result.stages.push(stage);
        if !passed {
            return Ok(result);
        }
    }
    result.spec_json = Some(spec_json.clone());
    if trace_json.exists() {
        result.decision_trace = Some(trace_json);
    }

    let schematic_py = work_dir.join("circuit.py");
    let stage = run_stage(
        "schematic",
        Command::new(lob_bin)
            .arg("schematic")
            .arg(&spec_json)
            .arg("--out")
            .arg(&schematic_py),
    )?;
    let passed = stage.passed();
    result.stages.push(stage);
    if !passed {
        return Ok(result);
    }
    result.schematic_py = Some(schematic_py.clone());

    let stage = run_stage("run", Command::new(lob_bin).arg("run").arg(&schematic_py))?;
    result.stages.push(stage);

    Ok(result)
}

fn run_stage(name: &str, cmd: &mut Command) -> Result<StageRun> {
    let command_str = format!("{cmd:?}");
    let output = cmd
        .output()
        .with_context(|| format!("running stage '{name}' ({command_str})"))?;
    Ok(StageRun {
        stage: name.to_string(),
        command: command_str,
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_run_passed_requires_exit_code_zero() {
        let ok = StageRun {
            stage: "x".into(),
            command: "x".into(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(ok.passed());

        let failed = StageRun {
            exit_code: Some(1),
            ..ok.clone()
        };
        assert!(!failed.passed());

        let killed = StageRun {
            exit_code: None,
            ..ok
        };
        assert!(!killed.passed());
    }
}
