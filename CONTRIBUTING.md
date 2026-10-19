# Contributing

## Ground rules

- Keen's binaries, decompiled code and data dumps never enter the repository, in
  a commit, an issue or a pull request. Addresses, offsets, type names and short
  facts about the engine in prose are fine. Keep local research material in
  `research/`, which git ignores.
- A mod changes building and living. It never touches combat, loot, experience
  or enemies.
- A hook that fails turns its feature off and logs why. It never takes the game
  or the server down: every hook body runs behind `catch_unwind`.
- Geometry is integer only, so the server and every client agree bit for bit.

## Setup

Rust stable with the MSVC toolchain on Windows. The pure crates
(`zone-geometry`, `keen-reflect`) also build and test on Linux.

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --release
```

CI runs the same three on Linux and Windows.

Tests against the real binaries run when `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE`
point at local copies of `enshrouded_server.exe` and `enshrouded.exe`. Run them
before touching `keen-reflect` or the build table.

## Supporting a new game build

The loader only hooks builds listed in `crates/loader/src/builds.rs`, keyed by
the executable's debug fingerprint. For a new build:

1. Print the fingerprint and system entries with
   `cargo run -p keen-reflect --example fingerprint -- <exe> upgrade_building_range add_buildzones_server add_buildzones_client`.
2. Find the engine helpers listed in the row (iterator, attribute lookup, zone
   size, change marking, seam check) in the new binary. Their names and roles
   are in `builds.rs`; matching the previous build's function bytes with
   addresses masked out finds most of them.
3. Add a row for the server and one for the client, and check every field offset
   the hooks use against the binary-gated tests.
4. Test on a dedicated server with a copy of a world before releasing.

## Pull requests

- One change per pull request, with tests for geometry and for anything that can
  be tested away from the game.
- Say how you tested it in game, if it touches a hook.
- Commit messages: a short imperative subject, then why.
