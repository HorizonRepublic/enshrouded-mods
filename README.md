# ensh-mods

Building mods for Enshrouded, for you and your friends on one server. Bases get
roomier and easier to build; combat, loot, experience and enemies stay exactly
as Keen made them.

The whole pack is one file, `powrprof.dll`. You, your friends and the server
install the same file.

## Install

1. Download `ensh-mods-v<version>.zip` from the
   [latest release](https://github.com/HorizonRepublic/enshrouded-mods/releases/latest).
2. In Steam, right-click Enshrouded and pick **Manage → Browse local files**.
3. Copy `powrprof.dll` from the archive into that folder, next to
   `enshrouded.exe`, and start the game.

The server you play on needs the mods too; [Running a server](#running-a-server)
covers that part. To remove the mods, delete `powrprof.dll`.

## Mods

| Mod                                                 | What changes in game                                                                                                                 |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| [Flame altar zones](docs/mods/flame-altar-zones.md) | Altars placed close together form one base whose zones overlap without losing area, and you can build across the seams between them. |

Every mod in the pack is on as soon as the file is in place.

## Compatibility

- **Game version:** Enshrouded 1.0, build `1269579`. After a game update the
  mods switch themselves off and the game plays vanilla until a release supports
  the new build. Nothing breaks in the meantime.
- **Everyone on the same release:** the server and every player.
- **Servers:** Windows, and Linux through Proton.
- **Other mods:** works next to EML (Enshrouded Mod Loader) and its data mods.

## Troubleshooting

The game writes `logs\ensh-mods.log` next to `enshrouded.exe`. About 15 seconds
after the game starts it should hold a line ending in `on` for each mod.

| The log says              | Meaning                                                          |
| ------------------------- | ---------------------------------------------------------------- |
| No `ensh-mods.log` at all | The game did not load the file: check where `powrprof.dll` sits. |
| `unknown game build`      | The game updated. Wait for the next release.                     |
| `... off: <reason>`       | That mod stepped aside and the game runs vanilla for it.         |

A bug report with the log attached goes to
[Issues](https://github.com/HorizonRepublic/enshrouded-mods/issues).

## Running a server

### Windows

Copy `powrprof.dll` next to `enshrouded_server.exe` and restart the server.
`logs\ensh-mods.log` should show `altar zones on` and
`building across seams on`.

### Linux (Proton)

Wine prefers its own `powrprof.dll`, so the server needs an override:

1. Copy `powrprof.dll` into the server folder. For the
   `mornedhels/enshrouded-server` image that is `/opt/enshrouded/server/`.
2. Set `WINEDLLOVERRIDES=powrprof=n,b` in the server's environment.
3. Restart the server and check `logs/ensh-mods.log`.

### Updating

Replace `powrprof.dll` and restart the server; players replace theirs and
restart the game. A running game keeps the version it started with.

## Contributing

Building from source, the code layout, supporting a new game build and making a
release are in [CONTRIBUTING.md](CONTRIBUTING.md). Report security issues as
[SECURITY.md](SECURITY.md) describes.

## Legal

Not affiliated with or endorsed by Keen Games. Enshrouded and its logos are
trademarks of Keen Games.

The repository holds only original code. It reads the game's own reflection data
at runtime to interoperate with it and contains none of Keen's code, data or
assets.

Keen's terms of use for Enshrouded (status 29.06.2026, § 3.3 f) forbid creating
or using mods. Installing these mods is your decision and your risk.

## Acknowledgements

The reflection descriptor and system record layouts in `keen-reflect` follow the
analysis published in [Ember](https://github.com/zachthedev/enshrouded-ember)
(MIT). Inline hooks use [MinHook](https://github.com/TsudaKageyu/minhook)
through the [`minhook`](https://crates.io/crates/minhook) crate.

## Licence

MIT, see [LICENSE](LICENSE). [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)
lists the third-party code in the released DLL.
