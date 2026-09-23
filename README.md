# Ant Colony

An ant colony where every ant asks [TypeSafe Jev](https://docs.typesafe.ai/introduction)
what to do next. You are the queen, and your only control is a text field: you type a
sentence, and twenty ants each decide for themselves what it means for them.

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

Without a key the game starts, but nothing moves: there is no rule-based fallback on
purpose, because a colony that kept working without the model would hide what the model
contributes.

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
