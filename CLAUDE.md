# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Language

All project content — code, identifiers, comments, documentation, commit messages — is
written in **English**.

The exception is `internals/`, which is git-ignored and never reaches GitHub: it is
internal working material and may be German. The spec `internals/ANTS.md` is German.

## Project status

Greenfield. The repo currently holds only a `cargo new` skeleton: `src/main.rs` is Hello
World, `Cargo.toml` has **no dependencies at all** (Bevy is not added yet), and there is
**no commit** on `master`.

The actual substance is **`internals/ANTS.md`** — the binding specification for the game:
an ant colony in which every ant receives its intentions from TypeSafe Jev, while the
player, as the queen, steers the colony solely through natural-language orders. **Read
`internals/ANTS.md` before doing any work**; everything below is only a condensed form of
its binding parts.

Per the milestone plan (`internals/ANTS.md` §7), step 1 is still outstanding. Keep the
order: the CORS test comes before everything else because it determines the architecture,
then the simulation without Jev, and only then the API client.

## Commands

```sh
cargo run                      # native, debug — Bevy is noticeably slow in debug builds
cargo run --release            # for anything that should run smoothly
cargo clippy --all-targets
cargo fmt
cargo test                     # no tests exist yet
cargo test <name> -- --exact --nocapture   # a single test
trunk serve                    # WASM dev server (Trunk is installed, Trunk.toml missing)
trunk build --release          # web build
```

Toolchain here: rustc 1.98.1, edition 2024, target `wasm32-unknown-unknown` installed.
`wasm-opt` is **not** installed but will be needed for shipping (`internals/ANTS.md` §5.4).

The Bevy version is not chosen yet. The neighbouring repos under `~/Desktop/git/`
(`rust-bevy-podracer`, `rust-bevy-nature-of-code`, `rust-bevy-cheeseball`) use Bevy
0.18–0.19, some with `bevy_egui`.

## Architecture: the binding rules

`internals/ANTS.md` §10 marks these as already decided. Do not quietly refactor them away
— they carry the purpose of the demo.

1. **Two layers.** The simulation (60 Hz: movement, pathfinding, pheromones, energy) is
   purely classical, with no model involved. The decision layer (~1 Hz per ant) is where
   Jev is used. Without the API the game keeps running in full as an ordinary ant
   simulation.
2. **The frame loop never waits for a response.** Requests run asynchronously and
   intentions come back into the world over a channel/event. This holds for all three
   sources.
3. **Jev returns intentions, not individual steps** ("fetch the leaf to the north-east",
   not "move one cell east"). The simulation carries them out itself over several seconds.
4. **The `choice` options are generated at runtime from what the ant can see, never
   hardcoded.** This is what stops Jev from picking a target that does not exist — the
   type-safety benefit and the core of the demo.
5. **Three equal decision sources** (`jev` live, `replay`, `classic`) behind one
   `DecisionSource` trait, not as a special case in the code. The "with Jev / without Jev"
   toggle is the demo's strongest argument; `classic` doubles as the fallback on low
   confidence, network failure, or a missing key.
6. **Question texts are an asset, not code.** `instructions` and `criteria` live in
   `assets/questions.ron` and are reloaded at runtime — rewording without recompiling.
   Refining those texts is the actual development effort.
7. **Thresholds belong in configuration**, not in the code (thinking interval, ant count,
   confidence fallback). They are measured rather than guessed: the confidence fallback
   sits at 0.22, derived on 2026-09-21 — see `internals/konzept.md` §10.4. Note that the
   API's `confidence` is the distance from pure chance normalised by the number of
   options, not the top probability.
8. **The base URL stays configurable**, so a proxy can be slipped in without a code change
   if CORS blocks direct calls.
9. **Questions within one request are evaluated independently** — decompose instead of
   chaining. The state carries only what the ant can see; larger states padded with
   irrelevant detail reduce accuracy.
10. **One codebase for native and WASM.** No threads and no native TLS in the browser:
    use `ehttp` rather than a tokio-bound client, and a minimal hand-written API client
    rather than `typesafe-rs`.
11. **The API key belongs to the player.** `localStorage`, never in a URL or a save game,
    with a delete button, validation via a tiny request at startup, and a visible counter
    for calls and estimated cost. Without a key the game starts in replay or classic mode,
    never with an empty input field.
12. **One JSONL format serving two purposes.** Each decision is one line (timestamp, ant
    id, truncated state, all probabilities, chosen option, latency) and is at once the
    replay source and the basis for evaluation.
13. **Peaceful tone.** Provisioning mechanics, not combat; ants grow tired and return
    home. You lose when the colony's stores run out.

## Module boundaries

The proposed tree is in `internals/ANTS.md` §6. What matters about it is the direction of
dependencies: **`decisions/` is the only layer that knows about Jev**, `api/` knows
nothing about ants, and neither `world/` nor `ants/` may depend on `api/`.
`decisions/state_builder.rs` (world → Jev state) and `decisions/options.rs` (what is
visible → `choice` criteria) are the translation points; `ants/classic.rs` holds the
pheromone rules that must also carry the game with no model at all.

## Open questions that govern the implementation

- **CORS is unresolved and blocking** (`internals/ANTS.md` §5.3). Check it with a `fetch`
  from the browser console before implementing. If blocked: a thin proxy that merely
  forwards the player's key, or a web build restricted to replay mode.
- **The API details in `internals/ANTS.md` §4 come from third-party sources, not first
  hand.** Verify the endpoint path and field names against `https://docs.typesafe.ai/api`
  instead of treating them as settled.
- **Rate limits are unknown.** Start with one request per ant; batch 5–8 ants (§4.4) only
  once limits actually bite.
- **Cost is real money** (§9): keep the thinking interval and ant count configurable, and
  prefer event-driven re-queries over a short fixed interval.
