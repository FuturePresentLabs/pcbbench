//! Drives a design-agent backend as a subprocess and captures its
//! artifacts. Today's only backend is legion-of-bom's `lob` CLI; nothing in
//! this module is legion-of-bom-specific beyond the concrete subcommand
//! names below — a different backend is a different set of `Command`
//! invocations, not a different architecture (this is the whole point of
//! going through a subprocess boundary rather than linking `lob` as a
//! library).
//!
//! The seam itself ([`Backend`]) is [`eval::Backend`] — shared with
//! cadbench and any future sibling harness. `Outcome`/`Error` are this
//! crate's own associated types; nothing about them is generic.

pub use eval::Backend;

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::task::{Check, Task};

/// Failures that stop a run before it can be scored.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("creating work directory {path}")]
    Workdir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("spawning stage {stage:?}: {program}")]
    Spawn {
        stage: String,
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("copying existing spec from {path}")]
    CopySpec {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// One subprocess run's outcome — exit status + captured output, kept for
/// the scorer and for a human to inspect after the fact.
#[derive(Debug, Clone)]
pub struct StageRun {
    pub stage: String,
    pub command: String,
    /// `None` if the process was killed by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl StageRun {
    #[must_use]
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
    /// Short identifier of the backend that produced this run (matches
    /// [`Backend::name`]) — threaded through to [`eval::ScoreReport::backend`].
    pub backend: String,
    pub stages: Vec<StageRun>,
    pub spec_json: Option<PathBuf>,
    pub schematic_py: Option<PathBuf>,
    pub decision_trace: Option<PathBuf>,
    pub panel_toml: Option<PathBuf>,
    pub board_kicad_pcb: Option<PathBuf>,
    /// `lob scope-probe`'s report, when a `Check::SpiceClips` criterion
    /// asked for it — absent whenever no task criterion needs it, same as
    /// every other optional artifact here.
    pub scope_probe_json: Option<PathBuf>,
}

/// Drives the `lob` CLI out of a legion-of-bom checkout (or any binary on
/// `PATH` matching its subcommand surface).
#[derive(Debug, Clone)]
pub struct LobBackend {
    /// Path to the `lob` binary.
    pub lob_bin: PathBuf,
    /// Skips the live decision-making step (`lob spec`, which needs a
    /// reachable SystemOne/Jev-compatible endpoint) and starts from an
    /// already-written spec JSON instead — for replaying or testing the
    /// rest of the pipeline without a network call, the same concept-vs-
    /// design split `lob schematic` itself is built around.
    pub existing_spec: Option<PathBuf>,
}

impl LobBackend {
    /// A backend driven through a `lob` binary.
    #[must_use]
    pub fn new(lob_bin: impl Into<PathBuf>) -> Self {
        Self {
            lob_bin: lob_bin.into(),
            existing_spec: None,
        }
    }

    /// Skips the design stage, starting from an already-written spec JSON.
    #[must_use]
    pub fn with_spec(mut self, existing_spec: impl Into<PathBuf>) -> Self {
        self.existing_spec = Some(existing_spec.into());
        self
    }

    /// Runs one `lob` subcommand, capturing everything it said.
    ///
    /// A non-zero exit is recorded, not raised: a backend that fails a
    /// stage is a result the rubric has an opinion about.
    fn stage(&self, name: &str, cmd: &mut Command) -> Result<StageRun, RunError> {
        let command_str = format!("{cmd:?}");
        let output = cmd.output().map_err(|source| RunError::Spawn {
            stage: name.to_owned(),
            program: self.lob_bin.display().to_string(),
            source,
        })?;
        Ok(StageRun {
            stage: name.to_owned(),
            command: command_str,
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

impl Backend<Check> for LobBackend {
    type Outcome = RunResult;
    type Error = RunError;

    fn name(&self) -> &str {
        "legion-of-bom"
    }

    /// Drive `lob` through concept → spec → schematic → run → board → drc
    /// for `task`, writing artifacts under `work_dir`.
    fn run(&self, task: &Task, work_dir: &Path) -> Result<RunResult, RunError> {
        std::fs::create_dir_all(work_dir).map_err(|source| RunError::Workdir {
            path: work_dir.display().to_string(),
            source,
        })?;
        let mut result = RunResult {
            backend: self.name().to_owned(),
            ..RunResult::default()
        };

        let spec_json = work_dir.join("design.json");
        let trace_json = work_dir.join("design.trace.json");

        if let Some(existing) = &self.existing_spec {
            std::fs::copy(existing, &spec_json).map_err(|source| RunError::CopySpec {
                path: existing.display().to_string(),
                source,
            })?;
        } else {
            let stage = self.stage(
                "spec",
                Command::new(&self.lob_bin)
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
        let panel_toml = work_dir.join("panel.toml");
        let stage = self.stage(
            "schematic",
            Command::new(&self.lob_bin)
                .arg("schematic")
                .arg(&spec_json)
                .arg("--out")
                .arg(&schematic_py)
                .arg("--panel")
                .arg(&panel_toml),
        )?;
        let passed = stage.passed();
        result.stages.push(stage);
        if !passed {
            return Ok(result);
        }
        result.schematic_py = Some(schematic_py.clone());
        if panel_toml.exists() {
            result.panel_toml = Some(panel_toml.clone());
        }

        if let Some(Check::SpiceClips {
            input_net_hint,
            min_amplitude_v,
            max_amplitude_v,
            ..
        }) = task
            .rubric
            .iter()
            .map(|c| &c.check)
            .find(|c| matches!(c, Check::SpiceClips { .. }))
        {
            let scope_probe_json = work_dir.join("scope-probe.json");
            let mut cmd = Command::new(&self.lob_bin);
            cmd.arg("scope-probe")
                .arg(&schematic_py)
                .arg("--min-amplitude")
                .arg(min_amplitude_v.to_string())
                .arg("--max-amplitude")
                .arg(max_amplitude_v.to_string())
                .arg("--out")
                .arg(&scope_probe_json);
            if let Some(hint) = input_net_hint {
                cmd.arg("--input-net-hint").arg(hint);
            }
            // Deliberately no early-return on failure here: a SPICE
            // measurement stage failing to run (crashed, no recognizable
            // input net) is worth recording, but board/DRC layout scoring
            // is an independent fact about the design and shouldn't be
            // blocked by it -- unlike spec/schematic/run/board, which each
            // other stage genuinely depends on.
            let stage = self.stage("scope-probe", &mut cmd)?;
            if stage.passed() && scope_probe_json.exists() {
                result.scope_probe_json = Some(scope_probe_json);
            }
            result.stages.push(stage);
        }

        let stage = self.stage(
            "run",
            Command::new(&self.lob_bin).arg("run").arg(&schematic_py),
        )?;
        let passed = stage.passed();
        result.stages.push(stage);
        if !passed {
            return Ok(result);
        }

        let board_kicad_pcb = work_dir.join("board.kicad_pcb");
        let stage = self.stage(
            "board",
            Command::new(&self.lob_bin)
                .arg("board")
                .arg(&schematic_py)
                .arg("--panel")
                .arg(&panel_toml)
                .arg("--out")
                .arg(&board_kicad_pcb),
        )?;
        let passed = stage.passed();
        result.stages.push(stage);
        if !passed {
            return Ok(result);
        }
        result.board_kicad_pcb = Some(board_kicad_pcb.clone());

        let stage = self.stage(
            "drc",
            Command::new(&self.lob_bin).arg("drc").arg(&board_kicad_pcb),
        )?;
        result.stages.push(stage);

        Ok(result)
    }
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

    #[test]
    fn backend_name_is_recorded() {
        let backend = LobBackend::new("/bin/lob");
        assert_eq!(backend.name(), "legion-of-bom");
    }
}
