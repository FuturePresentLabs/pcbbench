use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use pcbbench::runner::{Backend, LobBackend};
use pcbbench::scorer::score;
use pcbbench::task::Task;

#[derive(Parser)]
#[command(
    name = "pcbbench",
    version,
    about = "Eval harness for typed-decision-driven PCB design agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a task against a design-agent backend and score the result.
    Run {
        /// Task TOML file.
        task: PathBuf,
        /// Path to the backend binary (today: legion-of-bom's `lob`).
        #[arg(long)]
        lob: PathBuf,
        /// Working directory for artifacts (created if missing).
        #[arg(long)]
        work_dir: PathBuf,
        /// Skip the live decision-making step (`lob spec`, which needs a
        /// reachable SystemOne/Jev-compatible endpoint) and start from an
        /// already-written spec JSON instead.
        #[arg(long)]
        spec: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            task,
            lob,
            work_dir,
            spec,
        } => run_cmd(task, lob, work_dir, spec),
    }
}

fn run_cmd(
    task_path: PathBuf,
    lob: PathBuf,
    work_dir: PathBuf,
    spec: Option<PathBuf>,
) -> Result<()> {
    let text = std::fs::read_to_string(&task_path)
        .with_context(|| format!("reading {}", task_path.display()))?;
    let task: Task =
        toml::from_str(&text).with_context(|| format!("parsing {}", task_path.display()))?;

    println!("pcbbench run: {} ({})", task.id, task.family);

    let mut backend = LobBackend::new(&lob);
    if let Some(spec_path) = spec {
        backend = backend.with_spec(spec_path);
    }
    let result = backend
        .run(&task, &work_dir)
        .with_context(|| format!("running {} against {}", backend.name(), task.id))?;

    for stage in &result.stages {
        println!(
            "  [{}] {}",
            stage.stage,
            if stage.passed() { "ok" } else { "FAILED" }
        );
        if !stage.passed() {
            if let Some(line) = stage.stderr.lines().next_back() {
                println!("    {line}");
            }
        }
    }

    let report = score(&task, &result);
    println!("\nScore report for {}:", report.task_id);
    for r in &report.results {
        let mark = match r.verdict {
            eval::Verdict::Pass => "PASS",
            eval::Verdict::Fail => "FAIL",
            eval::Verdict::NeedsHuman => "?   ",
        };
        println!("  [{mark}] {} -- {}", r.description, r.detail);
    }
    if !report.needs_human().is_empty() {
        println!(
            "\nnote: criteria still need a human: {}",
            report.needs_human().join(", ")
        );
    }

    let report_path = work_dir.join("report.json");
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("writing {}", report_path.display()))?;
    println!("\nwrote {}", report_path.display());

    if report.all_automated_pass() {
        Ok(())
    } else {
        anyhow::bail!("not every automated criterion passed");
    }
}
