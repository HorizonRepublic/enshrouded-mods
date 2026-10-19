# Flame altar count

Needs the mod on the server and on every player's game, with the same
`ensh-mods.toml`.

## In vanilla

The number of Flame Altars in a world depends on its Flame level: 2 at level 1,
then 4, 6, 8, 10 and one more per level up to 16 at level 11. The whole world
shares them, so friends who each want a house and a few outposts run out fast.

## With the mod

The allowance comes from `altars_per_flame_level` in `ensh-mods.toml`. It holds
an entry per Flame level from level 0, and levels past the end use the last. The
default is twice vanilla, which reaches 16 at Flame level 4. The game groups at
most 16 altar zones into bases, so the mods lower higher values to 16.

The server decides where an altar may go. A player whose file differs sees a
different number in the interface than the server allows.

## Log lines

- Server and players: `altar count on: [...] by Flame level`, with the table in
  use.
- `ensh-mods.toml: ...` warns about a line that was ignored and why.
