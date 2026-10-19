# ensh-mods

Mods for a private Enshrouded server: building and living get more comfortable,
while combat, loot, experience and enemies stay vanilla.

All mods live in one file, `powrprof.dll`, which the dedicated server and every
player's game load alike.

## Mods

### Flame altar zones

Altars whose zones touch or overlap form a cluster, whose area is the sum of its
altars' zones: what the overlap took comes back as a band of equal width around
the whole cluster. Placing or upgrading an altar never shrinks a zone, and
objects can go across the seams between altars of one cluster.

The cluster's outline follows the altars' boxes, so three altars of different
sizes give a stepped shape, not one rectangle.

## Supported game build

Enshrouded 1.0, build `1269579` (Steam build `25988414`), server and client.

On any other build the mods switch themselves off and the game runs vanilla; the
log says why. After a game update, wait for a release that adds the new build.

## Installing

Every player and the server need the same release.

### Players

1. Download `ensh-mods-v<version>.zip` from the repository's Releases page.
2. In Steam, right-click Enshrouded, then **Manage → Browse local files**.
3. Copy `powrprof.dll` from the archive into that folder, next to
   `enshrouded.exe`.
4. Start the game; within about 15 seconds `logs\ensh-mods.log` in the same
   folder shows `building across seams on`.

To remove the mods, delete `powrprof.dll`. Steam's file verification leaves it
in place.

### Dedicated server on Windows

Copy `powrprof.dll` next to `enshrouded_server.exe` and restart the server.
`logs\ensh-mods.log` shows `altar zones on` and `building across seams on`.

### Dedicated server on Linux (Proton)

Wine prefers its own `powrprof.dll`, so the server needs an override:

1. Copy `powrprof.dll` into the server folder, for the
   `mornedhels/enshrouded-server` image that is `/opt/enshrouded/server/`.
2. Set `WINEDLLOVERRIDES=powrprof=n,b` in the server's environment.
3. Restart the server and check `logs/ensh-mods.log`.

### Next to EML

The mods coexist with EML (Enshrouded Mod Loader), which uses `dbghelp.dll` or
`dinput8.dll`.

## Updating

Replace `powrprof.dll` and restart the game or the server. A running game keeps
the version it started with.

## Building from source

Needs Rust stable with the MSVC toolchain on Windows.

```sh
cargo build --release -p loader   # target/release/powrprof.dll
cargo test
```

Tests against the real game binaries run when `ENSH_SERVER_EXE` and
`ENSH_CLIENT_EXE` point at local copies of `enshrouded_server.exe` and
`enshrouded.exe`.

| Crate                  | Holds                                                       |
| ---------------------- | ----------------------------------------------------------- |
| `crates/zone-geometry` | Cluster geometry, independent of the game                   |
| `crates/keen-reflect`  | Engine reflection and system records read from a game image |
| `crates/loader`        | The proxy DLL, the build table and the hooks                |

Keen's binaries, decompiled code and data dumps never enter the repository.
`BRIEF.md` holds the design and the engine facts.

## Legal

Not affiliated with or endorsed by Keen Games. Enshrouded and its logos are
trademarks of Keen Games.

The repository holds only original code. It reads the game's own reflection data
at runtime to interoperate with it and contains none of Keen's code, data or
assets.

Keen's terms of use for Enshrouded (status 29.06.2026, § 3.3 f) forbid creating
or using mods. Installing these mods is your decision and your risk. They change
building only: combat, loot, experience and enemies stay vanilla.

## Acknowledgements

The reflection descriptor and system record layouts in `keen-reflect` follow the
analysis published in [Ember](https://github.com/zachthedev/enshrouded-ember)
(MIT). Inline hooks use [MinHook](https://github.com/TsudaKageyu/minhook)
through the [`minhook`](https://crates.io/crates/minhook) crate.

## Licence

MIT, see `LICENSE`. `THIRD-PARTY-NOTICES.md` lists the third-party code in the
released DLL.

## Releasing

1. Set `version` in `crates/loader/Cargo.toml`.
2. Commit, then tag and push:
   `git tag v<version> && git push origin v<version>`.
3. The `Release` workflow tests, builds and publishes `ensh-mods-v<version>.zip`
   with `SHA256SUMS`.
