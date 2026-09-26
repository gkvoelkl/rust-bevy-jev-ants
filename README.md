# Ant Colony

An ant colony where every ant asks [TypeSafe Jev](https://docs.typesafe.ai/introduction)
what to do next. You are the queen, and your only control is a text field: you type a
sentence, and twenty ants each decide for themselves what it means for them.

**▶ [Play it in the browser](https://gkvoelkl.github.io/rust-bevy-jev-ants/)** — bring
your own [TypeSafe key](https://console.typesafe.ai/keys); the page asks for it and uses
it for that one visit. Without a key nothing moves, and that is on purpose.

![Level 1: eight ants fetching fruit, each label an intent from Jev](/docs/screenshot.png)

Above every ant is the intent it chose and how sure the model was — green when it is
certain, red when it barely cleared the threshold. `scent south-west 0.77` is an ant that
decided to follow a pheromone trail home; the faint green cells are those trails, a road
between fruit and nest that nobody planned. That picture is the point of the whole
thing.

## Run it

You need [Rust](https://rustup.rs) and a TypeSafe API key from
[console.typesafe.ai](https://console.typesafe.ai/keys).

```sh
cp .env.example .env     # then put your key in it
cargo run --release
```

Type an order into the field at the bottom and press Enter. Until then the colony sleeps
in its nest and not a single request goes out.

`.env` is optional. Started without a key, the game asks for one in a dialog before
anything else — that key is used for this run only and is never written down, so the next
start asks again.

There is no way past that dialog but a key, and no rule-based fallback behind it. That is
on purpose: a colony that kept working without the model would hide what the model
contributes, so without a key there is nothing to play.

### In the browser

```sh
python3 tools/dev-proxy.py    # in one terminal
trunk serve --release         # in another — http://localhost:8080
```

Same game, same code — no file system, so the boards and the question text are compiled
in, and the key is always asked for in the dialog.

Two commands, not one, because of the API. A page cannot call it at all: the CORS
preflight is refused from every origin tried on 2026-09-24, its own console included, so
the browser never sends the request. The web build therefore calls `/api/...` on its own
origin and lets the server forward it. But the API reads `Origin` on the forwarded request
too, and refuses one it does not know — and a browser attaches `Origin` to every POST. So
the thing that forwards has to drop that header, which `Trunk.toml` cannot; that is all
`tools/dev-proxy.py` does.

A deployment with a web server of its own needs neither command, just one rule: forward
`/api` and drop the `Origin` header while doing it (nginx: `proxy_pass` plus
`proxy_set_header Origin "";`). Nothing in the game changes. The key travels in the
`Authorization` header, is only passed through, and is never stored at either end.

```sh
trunk build --release         # just the bundle, in dist/
```

The bundle is 38 MB, about 12 MB over the wire once the server compresses it — serve it
with gzip or brotli on. Bevy's 3D pipeline and audio stack are switched off in
`Cargo.toml`, since this game is sprites and text; that alone was 9 MB.

### On GitHub Pages

`.github/workflows/pages.yml` builds the bundle on every push to `master` and publishes
it, so the link at the top of this page is always the current game. The 40 MB of
WebAssembly is built there and never committed — a binary that size in the history would
be paid for on every clone, forever.

A first visit pulls the whole colony down: 38 MB of WebAssembly, 12.5 MB of it over the
wire — Pages serves `application/wasm` gzipped, measured 2026-09-26.

Pages serves files and nothing else, though, so the rule above has nowhere to live: there
is no `/api` on that origin to forward anything. The proxy therefore stands on its own,
as a Cloudflare Worker — `tools/worker.js`, about twenty lines, free plan, no card. Being
on another origin it has to answer the CORS preflight itself as well, which it can,
since it is ours.

```sh
cd tools && npx wrangler deploy                             # prints the Worker's URL
gh variable set ANTS_API_BASE --body https://….workers.dev  # the build reads it
gh workflow run "Play it on Pages"
```

`ANTS_API_BASE` is baked into the bundle at build time by `api::DEFAULT_BASE_URL`, and it
is the only difference between the deployed game and the one `trunk serve` builds. A
build without it stops with a message rather than deploying a board that loads, asks for
a key and then fails every request.

The Worker forwards one path and holds nothing: no key of its own, no state, and no log
of what passes through. The player's key still travels through someone else's machine on
its way to the API, which is worth knowing before typing it into a hosted page — running
the game locally keeps it between the terminal and `api.typesafe.ai`.

## Playing

* **Type a sentence.** "Spread out to the east", "bring home everything you carry",
  "stay together". Any language works — it is handed to Jev word for word.
* **Click an ant** to see the exact request that went out for it, the options it was
  offered, and the answer as it came back.
* **F2** opens the question itself. The text every ant is asked lives in
  `assets/questions.ron`, and you can reword it while the game runs — the next round
  answers differently. That is the actual experiment.
* **F3** hides the intent labels.
* **Level buttons** at the bottom switch boards; **Restart** plays the same one again,
  which is what you want after a sentence that did not work.

Two boards come with it: fruit scattered over open ground, and a river that has to be
bridged with a plank two ants must carry together.

## How it works

Two layers, and they never wait for each other.

* **The simulation runs at 60 Hz and is entirely classical.** Movement, pathfinding,
  pheromone trails, collisions, stores — no model involved.
* **The decision layer asks Jev about every two seconds per ant.** The request goes out,
  the frame carries on, and the answer comes back over a channel later.

Jev returns *intents*, not single steps: "fetch the fruit two cells to the north-east",
which the simulation then carries out over several seconds.

The options in each request are **built at runtime from what that one ant can see**. An
ant with nothing in sight is not offered `fetch_north_east`, so the model cannot choose a
fruit that does not exist. When the answer comes back it is checked twice — the key must
be one that was offered, and the confidence must clear 0.22 — and if either check fails
the ant simply does nothing that round. Discarded answers are counted and shown.

One ant's request looks like this:

```json
{
  "model": "jev-latest",
  "state": {
    "order_from_the_ant_queen": "Bring fruit home",
    "vision": "This ant sees 3 cells in every direction.",
    "nearby": ["a fruit, 2 cells to the north-east", "another ant, 1 cell to the south"]
  },
  "questions": {
    "step": {
      "type": "choice",
      "instructions": "Which single step should this ant take now? Follow the ant queen's order whenever it applies.",
      "criteria": {
        "fetch_north_east": "Fetch the fruit 2 cells to the north-east",
        "north": "Head north for a few cells",
        "east": "Head east for a few cells",
        "west": "Head west for a few cells"
      }
    }
  }
}
```

## Command line

```sh
cargo run -- --order "Go east"         # start with an order already given
cargo run -- --scenario plank          # start on a particular board
cargo run -- --probe                   # one real request, printed — costs a fraction of a cent
cargo run -- --measure                 # measure confidence and cost against the live API
cargo run -- --compare                 # simulation only, no requests at all
```

## Layout

```
src/world/      the board: grid, fruit, nest, pheromones, rendering
src/ants/       the ants: movement and the intents they pursue
src/decisions/  the only layer that knows about Jev
src/api/        a minimal TypeSafe client that knows nothing about ants
assets/         the question text and the levels
```

Built with [Bevy](https://bevyengine.org) 0.19 and `bevy_egui`. One codebase for native
and WASM.

## License

MIT — see [LICENSE](LICENSE).
