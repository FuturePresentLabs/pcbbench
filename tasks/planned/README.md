# Planned tasks

Target specs for eval tasks in families legion-of-bom's curated library
doesn't support yet — today that's just `fuzz-pedal`
(`Topology::Silicon2TransistorFuzz`, now composable into variable-length
gain-stage chains via `lob spec-chain`, but still one circuit family).

These files use the same schema as `tasks/*.toml` but live one directory
deeper on purpose:

- `scripts/update-badges.sh` only counts `tasks/*.toml` (`-maxdepth 1`), so
  these don't inflate the eval badge with tasks that can't actually run.
- `every_shipped_task_parses_and_has_a_sound_rubric` only reads `tasks/`
  itself, not subdirectories, so these aren't asserted against as if they
  were real, currently-passing evals.

Move a file up into `tasks/` (and drop its `# PLANNED` header comment) once
its `family` exists as a real curated topology and a live run has actually
been scored against it — matching this project's own rule that a task file
is the harness's contract, not sample data.

Current planned families:

- **`lc-lowpass-filter`** — a simple RF task (third-order LC low-pass,
  passive only). The first "not fuzz-pedal" family worth targeting: no
  active-component decisions, small node count, a good stress test for
  whether the topology-DAG approach generalizes past gain-stage circuits.
- **`linear-regulator`** — a simple general-EE task, deliberately
  unambiguous (a positive control once the family exists).
- **`stm32-codec`** — an STM32H743 audio board with one of three codec
  options (PCM5102A + PCM1808, WM8731, ES8388), decided from the brief. The
  first digital-board family: a 0.5 mm-pitch LQFP-100, cited decoupling, a
  crystal, SWD pads, I2S/SAI. Three tasks, each written so a different codec
  is the fitting answer (no control bus / a microphone input / headphones).
  Exists in legion-of-bom (`crates/core/src/mcu_audio.rs`); not yet scored end
  to end — the board stage's layout of a 100-pin LQFP is the open question.
