# Planned tasks

Target specs for eval tasks in families legion-of-bom's curated library
doesn't support yet, or supports but hasn't scored end to end yet. The live
tasks are all `fuzz-pedal` (`Topology::Silicon2TransistorFuzz`, composable
into variable-length gain-stage chains via `lob spec-chain`).

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
- **`board`** — boards synthesized from legion-of-bom's parts catalog by
  typed decisions (`lob spec board`), with SKiDL as the IR. The family
  exists and every brief below passes `lob run` live; none is scored through
  `board` + `drc` yet.
  - `board-stm32-codec-*` — an STM32H743 audio board, each brief written so
    a different codec is the fitting answer (PCM5102A + PCM1808 with no
    control bus / WM8731 for a microphone input / ES8388 for headphones).
    The board stage's layout of a 100-pin LQFP is the open question.
  - `board-subghz-telemetry-v1` — the first RF board: an STM32 and a
    sub-GHz radio with its cited matching network, an SMA port, USB-C power.
