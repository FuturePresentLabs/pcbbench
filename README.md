# PCBBench

[![license](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](#license)
[![tests](https://img.shields.io/badge/tests-14%20passing-brightgreen.svg)](#status)
[![evals](https://img.shields.io/badge/evals-20-orange.svg)](#tasks)
[![status](https://img.shields.io/badge/status-runs%20end%20to%20end-success.svg)](#status)

*(Badge numbers are generated — run `scripts/update-badges.sh` after a test or task count changes; don't hand-edit them.)*

An eval harness for **typed-decision-driven PCB/hardware design agents**.

Most "AI hardware design" demos are hard to trust because the model is free
to say anything — a plausible-sounding schematic is not the same as a
checked one. PCBBench takes the opposite bet: a design agent should make a
small number of **bounded, typed decisions** (a choice from a known set, a
calibrated yes/no probability, a position on an ordered rubric — never free
text), and everything downstream should be a deterministic, replayable
function of exactly those decisions. PCBBench runs that pipeline end to end
and scores what comes out against a rubric, so "does this design agent
actually work" has an answer that isn't a vibe.

## How it fits together

```
concept (a design brief)
   │
   ▼  typed decisions (Jev / typesafe.ai "System One"-compatible: choice / noul / score)
spec  ──────────────────────────────────────────────►  a spec file — decisions in, no schematic
   │
   ▼  pure rendering, no further decisions, no network
design (schematic → board → panel → DRC)
   │
   ▼
PCBBench scores the artifacts against a task's rubric
```

The spec/design split is deliberate. A spec is *just* the decisions (raw
text + machine-readable JSON) — replayable and inspectable on their own.
The schematic renderer is a pure function of a spec file: running it twice
on the same spec always produces the same circuit, because nothing about
the design is decided a second time.

PCBBench itself is **backend-agnostic**: it drives a design-agent backend
as a subprocess (`runner`) and scores the artifacts that come back
(`scorer`). Today's only backend is
[legion-of-bom](https://github.com/FuturePresentLabs/legion-of-bom)'s `lob`
CLI (`lob spec` → `lob schematic` → `lob run`), but nothing outside a
handful of `Command` invocations in `runner.rs` knows that — a different
backend is a different set of subprocess calls, not a different
architecture.

## Status

**Runs end to end.** 10 unit tests, 20 tasks, one backend (legion-of-bom).
The runner wires the full `spec → schematic → run → board → drc` chain,
deriving the panel TOML from the spec's enclosure-size decision along the
way — so `drc_clean` is a real pass/fail against a board legion-of-bom
actually placed and routed, not a "did not run."

We're planning to publish real benchmark results here as the design-agent
and rubric mature — this repo being public from the start is part of that.

## Running it

```sh
cargo build --release
./target/release/pcbbench run tasks/fuzz-pedal-v1.toml \
  --lob /path/to/lob \
  --work-dir ./out/fuzz-pedal-v1
```

`--spec <path>` skips the live decision-making step (`lob spec`, which
needs a reachable Jev/System One-compatible endpoint) and replays an
already-written spec JSON instead — useful for iterating on the scorer or
the rest of the pipeline without spending a live decision call each time.

## Tasks

20, all under `tasks/`, one circuit family (`fuzz-pedal`, the only topology
legion-of-bom's curated library has today — a silicon two-transistor
common-emitter fuzz). Every task varies the *brief*, stressing one of two
real typed decisions the backend makes: bias voice (bright/symmetric/dark)
and enclosure size. Grouped by what each one is testing:

**Baseline & positive controls** — voice and enclosure both stated plainly,
should score confidently:

- **`fuzz-pedal-v1`** — the original baseline, terse.
- **`fuzz-pedal-explicit-voice-v1`** — bright + smallest enclosure, named
  directly.
- **`fuzz-pedal-garage-rock-v1`** — bright/cutting + smallest enclosure, via
  a 60s-garage framing.
- **`fuzz-pedal-doom-stoner-v1`** — dark/thick + larger enclosure, via a
  downtuned-baritone framing.
- **`fuzz-pedal-budget-build-v1`** — bright + smallest enclosure, via a
  first-build/cost-conscious framing.
- **`fuzz-pedal-boutique-premium-v1`** — dark/vintage + standard enclosure,
  via a no-cost-cutting framing.
- **`fuzz-pedal-fuzzface-lineage-v1`** — names the curated topology's actual
  mechanism directly (silicon, two-transistor, rail-clipping); a sanity
  check that a brief matching reality scores well.

**Indirect voice framing** — same three voice targets, signaled through
genre/reference language rather than named:

- **`fuzz-pedal-blues-warm-v1`** — warm/touch-sensitive, blues framing.
- **`fuzz-pedal-metal-cut-v1`** — bright/cutting, metal framing.
- **`fuzz-pedal-shoegaze-wall-v1`** — dark/smeared, ambient framing.
- **`fuzz-pedal-symmetric-balanced-v1`** — even-handed, stated directly.
- **`fuzz-pedal-indie-versatile-v1`** — even-handed, via a versatility
  framing.

**Stress cases** — deliberately hard, expected to strain the confidence
criterion or a stage:

- **`fuzz-pedal-ambiguous-voice-v1`** — "warm/vintage" and
  "aggressive/cuts through" in the same brief, pulling bias-voice in
  opposite directions.
- **`fuzz-pedal-enclosure-contradiction-v1`** — "as compact as possible"
  and "room to work inside," pulling enclosure-size in opposite directions.
- **`fuzz-pedal-germanium-out-of-scope-v1`** — asks for a germanium
  three-transistor circuit; the curated library only has silicon
  two-transistor. Expected to surface as a capability mismatch, not a
  silent substitution.

**Signal-to-noise** — real signal present but buried in verbosity, or
absent entirely:

- **`fuzz-pedal-tour-story-verbose-v1`** — bright/compact, buried in
  touring-band backstory.
- **`fuzz-pedal-pedalboard-chain-verbose-v1`** — dark/roomier, buried in
  signal-chain context.
- **`fuzz-pedal-terse-aggressive-v1`** / **`fuzz-pedal-terse-dark-v1`** —
  one-sentence briefs, minimal signal.
- **`fuzz-pedal-trust-the-builder-v1`** — explicitly defers the voice
  decision entirely ("you pick").

## Task format

A task is a design brief plus a rubric:

```toml
id = "fuzz-pedal-v1"
family = "fuzz-pedal"
brief = "vintage silicon fuzz, 9V, true bypass"

[[rubric]]
id = "drc"
description = "the generated board is DRC-clean"
kind = "drc_clean"

[[rubric]]
id = "vibe"
description = "reads as a designed product, not generated"
kind = "subjective"
```

Every rubric criterion is either **objective** (`stages_pass`,
`min_decision_confidence`, `drc_clean`, `spice_clips` today — a pure
function of captured artifacts, no human judgment) or **`subjective`**
(explicitly not automated yet, so a task file is honest about what it
can't verify itself). The goal is for that second category to shrink over
time, not to stay a permanent escape hatch.

### `spice_clips`, and its known gap

```toml
[[rubric]]
id = "clips"
description = "input sweep shows real clipping, not a schematic-reading guess"
kind = "spice_clips"
min_amplitude_v = 0.1
max_amplitude_v = 2.0
min_flat_top_at_max = 0.8
```

A sine sweep driven into the rendered circuit's input net, scored on the
flat-topping fraction at `max_amplitude_v` — legion-of-bom's own
`scope_probe`/`simulate_tran_drive`, live-validated against real fuzz
circuits (asymmetric BJT clipping, onset ~100-300mV, 80-96% flat-top by
2V). `input_net_hint` is optional.

**Known gap, same honesty `cadbench` holds `conforms` to**: the runner
shells out to a `lob scope-probe` subcommand that does not exist in
legion-of-bom yet (`lob spec`/`schematic`/`run`/`board`/`drc` do). Only
invoked when a task's rubric actually declares `spice_clips` — no shipped
task does yet. Expected contract:
`lob scope-probe <schematic.py> [--input-net-hint NAME] --min-amplitude V --max-amplitude V --out <report.json>`,
exit 0 whenever the measurement itself ran (regardless of what it found),
writing `{"flat_top_at_max": <f64>, ...}` to `--out`. The stage
deliberately doesn't gate `run`/`board`/`drc` — a SPICE measurement
failing to run is an independent fact from whether the board layout
succeeds.

## Alternatives

Other benchmarks in this space, for context and future comparison — not
wired up here, listed so a future "how does `lob` compare" question has
somewhere real to point:

- **[HWE-Bench](https://arxiv.org/pdf/2603.18102)** — 300 board-level
  schematic-design tasks sourced from GitHub/OSHWLab across 8 application
  domains, backed by a 2,914-datasheet knowledge base, scored by electrical
  rules checking followed by circuit simulation. Top reported model: an
  8.15% pass rate — a genuinely hard bar.
- **PCB-Bench** (academic) — ~3,700 text questions, 500 image-text
  questions, 174 real projects.
- **PCBWorld** — KiCad-engine-grounded routing with a DRC-feedback loop;
  closest shape match to this repo's own `spec → schematic → drc` pipeline.

## License

AGPL-3.0-or-later.
