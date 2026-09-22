# PCBBench

[![license](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](#license)
[![tests](https://img.shields.io/badge/tests-8%20passing-brightgreen.svg)](#status)
[![status](https://img.shields.io/badge/status-runs%20end%20to%20end-success.svg)](#status)

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

**Runs end to end.** 8 unit tests, three tasks, one backend
(legion-of-bom). The runner wires the full `spec → schematic → run → board
→ drc` chain, deriving the panel TOML from the spec's enclosure-size
decision along the way — so `drc_clean` is a real pass/fail against a board
legion-of-bom actually placed and routed, not a "did not run."

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

Three, all under `tasks/`, one circuit family (`fuzz-pedal`) with different
briefs stressing different parts of the rubric:

- **`fuzz-pedal-v1`** — the baseline: terse, minimally specified.
- **`fuzz-pedal-explicit-voice-v1`** — voice and enclosure both stated
  plainly. A positive control: this should score high on confidence.
- **`fuzz-pedal-ambiguous-voice-v1`** — asks for "warm and vintage" *and*
  "aggressive, cuts through a mix" in the same brief. Those pull the
  bias-voice decision in opposite directions on purpose — a stress test for
  the confidence criterion.

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
`min_decision_confidence`, `drc_clean` today — a pure function of captured
artifacts, no human judgment) or **`subjective`** (explicitly not automated
yet, so a task file is honest about what it can't verify itself). The goal
is for that second category to shrink over time, not to stay a permanent
escape hatch.

## License

AGPL-3.0-or-later.
