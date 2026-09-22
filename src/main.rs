use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use pcbbench::task::Task;
use pcbbench::{runner, scorer};

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

fn run_cmd(task_path: PathBuf, lob: PathBuf, work_dir: PathBuf, spec: Option<PathBuf>) -> Result<()> {
    let text = std::fs::read_to_string(&task_path)
        .with_context(|| format!("reading {}", task_path.display()))?;
    let task: Task =
        toml::from_str(&text).with_context(|| format!("parsing {}", task_path.display()))?;

    println!("pcbbench run: {} ({})", task.id, task.family);
    let result = runner::run(&lob, &task, &work_dir, spec.as_deref())?;
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

    let report = scorer::score(&task, &result);
    println!("\nScore report for {}:", report.task_id);
    let (passed, total) = report.objective_pass_count();
    println!("  objective: {passed}/{total} passed");
    for r in &report.results {
        let mark = match r.passed {
            Some(true) => "PASS",
            Some(false) => "FAIL",
            None => "?   ",
        };
        let kind = if r.objective { "objective" } else { "subjective" };
        println!("  [{mark}] ({kind}) {} -- {}", r.description, r.detail);
    }

    let report_path = work_dir.join("report.json");
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("writing {}", report_path.display()))?;
    println!("\nwrote {}", report_path.display());
    Ok(())
}
