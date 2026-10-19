# Brief

Mods for our own Enshrouded server: four players, a group that wants to build
separate houses close together. The repository is private for now and may go
open source later. Quality over speed: no throwaway mods.

## Principles

- Fair play. A mod makes building and living more comfortable; it never
  touches combat, loot, experience or enemies.
- Every player installs the mods. Client-side changes are acceptable.
- A mod, once enabled on the world, stays enabled. Removing it is not a
  supported path, though it must never corrupt a save.
- A broken hook disables its feature and keeps the server running; it never
  crashes the game.
- Testing in-game is done by people in a test world. Driving the game client
  through screenshots was tried before and is too slow to be useful.
- Keen's code and data never enter the repository: no decompiled code, no
  data dumps, no binaries. Local research material lives in the ignored
  `research/` directory.

## Mods

### Flame altar zones (first, main)

Problem: altars placed edge to edge so their zones can be built in grow into
each other when upgraded, and the overlapping volume is lost. More altars
alone do not help for the same reason.

Design, the cluster model:

1. Altars whose vanilla zones touch or overlap form a cluster.
2. A cluster's area budget is the sum of its altars' vanilla areas, so overlap
   is never lost.
3. The cluster's shape is the union of its vanilla zones grown uniformly
   outward in voxel-grid steps until the area meets the budget. Its height is
   the tallest member's.
4. Clusters whose grown shapes touch merge and are recomputed until nothing
   changes; the count only ever decreases, so it terminates.
5. Anything may be placed anywhere inside a cluster, across the seams between
   altars.
6. The result depends only on the set of altars (positions, levels), never on
   the order they were placed in. An altar placed inside an existing zone adds
   its budget to that cluster.
7. Upgrades keep their meaning: a higher altar level raises its vanilla area
   and so the cluster's budget.
8. The altar count per Flame level is raised by a configurable multiplier
   (the engine also caps it at 16 in code, see below).

The server and every client compute clusters independently from replicated
altar positions and levels, with the same pure function. A join check refuses
a client whose mod is missing or a different version, and says why.

Decided (2026-10-19):

- Growth is uniform in all directions, never only into free space.
- Every altar keeps one world-aligned box, so the engine and the save still
  see one box per base. Growing the union of boxes by k cells on every side
  equals growing each box by k cells, so the cluster is exactly the union of
  its members' boxes, each grown by the same k.
- k is rounded to the nearest step: with the budget between the areas at k
  and k + 1, less than half of that step rounds down to k, half or more
  rounds up to k + 1. The area therefore misses the budget by at most half a
  step in either direction, and k is never below 0.
- Each member's height becomes the tallest member's, grown around its own
  centre; k does not apply to height.
- Geometry is integer only, in voxel-grid cells. Boxes are half-open, like
  the engine's point test (`min <= p < max`), so two boxes touch when one's
  max equals the other's min on an axis and their ranges meet on the other.
- Clusters of different players merge like any other. Altars are not owned
  by a player; build rights come from the server's user groups.
- The altar multiplier is a fixed config value applied to
  `altarsPerFlameLevel`; the hook on the cap of 16 raises the cap to the
  multiplied table, so the multiplier also works at the highest levels.

Open:

- Adding an altar can shrink a cluster. The new member raises the budget by
  its own area, but every member grows by k, so the union can overshoot and
  k drops. A random search found this in about 6% of additions, by 2 to 6
  cells (1 to 3 m) on every side. Whatever sits in the lost strip falls out
  of the base, and water and terrain edits there are not saved. Choose
  between an exact budget and boxes that never shrink when an altar is
  added; both must stay independent of placement order.
- More than 16 altars in the world: the engine groups only the first 16
  zones into bases (see Engine). Until that is hooked, the multiplier must
  keep the world total at 16 or below.
- `upgrade_building_range` rewrites `TriggerShape.range*` every tick when it
  differs from the level's size, so a change made from outside is reverted
  on the next tick. The cluster size has to be written by a hook on that
  system, not set once.
- The seam check `0x14093e210` tests only two corners. Its replacement must
  test that the whole box lies in the union of the cluster's boxes: two
  corners in two different members' boxes do not prove that.

### Later

- Building blocks and decor: double yield per craft (preferred over halving
  the cost, so an unmodded display never shows a cost the server will not
  charge).
- Furniture comfort raised, so each house reaches the rested bonus.
- Storage: vanilla 1.0 already names chests, converts them to Magic chests
  that feed nearby workshops, and has the Quick Stash Station. Automatic
  consolidation between chests needs runtime hooks; revisit once the hook
  layer exists.

Rejected: an NPC per house (houses are close, one base serves everyone),
experience, loot and enemy changes.

## Game facts

Build `1269579|release_1.0|2026-10-16`, dedicated server app 2278520.

### Data

`keen::BalancingTable`, guid `82706b40-61b1-4b8f-8b23-dcec6971bda1`:

- `altarsPerFlameLevel`, u8 array indexed by Flame level (1 to 11):
  2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16; later entries stay 16.
- `buildzoneSizesPerAltarLevel`, six float3 sizes indexed by altar level:
  0, 40, 80, 120, 160, 160, with x, y and z separate. Most likely metres and
  the full edge, not a radius; unverified in game.
- `altarLevelMax = 5`, while the recipes stop at level 4.

### Engine (server exe, static analysis)

Addresses are for this build at the default image base `0x140000000`.

- Zones are rebuilt every tick by ECS systems, found by name in the binary's
  system table:
  - `clear_buildzones` empties the list.
  - `upgrade_building_range` (`0x140090a60`) writes half the level's size into
    the altar's `TriggerShape.rangeX/Y/Z`, marking the component changed
    through `0x140666a90` first.
  - `add_buildzones_server` (`0x1400e90e0`) turns every entity with
    `BaseIdComponent`, `CurrentTransform` and `TriggerShape` into a zone
    (`0x140909de0`). `add_buildzones_client` exists only in the client binary.
- A zone is a world-aligned box in 64-bit fixed point: transform plus
  `TriggerShape.offset`, half-extents `rangeX/Y/Z`, enclosed in an AABB.
- Placement checks:
  - `0x14093e210`: both corners of an object inside the same zone. This is
    the seam rule the cluster model replaces.
  - `0x14093e8d0`: box intersects any zone (permission check).
  - `0x14093e6b0`: point in one zone; `0x14093e180`, `0x14093e2a0`: point in
    any zone, with and without margin.
  - `0x140099d80`: cursor validation, returns `keen::ecs::CursorFlag`. Altars
    need `CanExtendBase`; `MaxAltarCountReached` compares against
    `altarsPerFlameLevel`, capped at 16 in `0x140931860`.
- Vanilla has no rule against overlapping altar zones, and it already
  merges zones that overlap or touch into one base: `0x140926410` collects
  {altar GUID, box}, `0x1407b0600` sorts by GUID, `0x1401ffb90` groups. Only
  the first 16 zones are grouped; a 17th zone belongs to no base.
- Placed objects carry no base id. `BaseIdComponent` is a GUID on altars
  only, written by `base_id_generator` (`0x1400eb020`). Comfort, crafting,
  beds and animals find their base every tick by the entity's pivot point
  (`0x1402022a0`), so nothing is reassigned when zones change. The seam
  check is the only rule that ties an object to a single altar's box.
- Zone coordinates are 32.32 fixed point (2^32 per metre). A voxel cell is
  0.5 m.
- The save stores one record per altar: GUID, box in whole 0.5 m cells,
  terrain edits and water (`0x1407b81b0`). Water is saved for the box only
  and loaded only when its size matches the record's box (`0x1408b9ffe`),
  which holds unless the box is off the 0.5 m grid. Growing a box keeps its
  water; shrinking loses whatever falls outside. The save grows with the
  zones' area: a copy of the live world saved as 190 KB vanilla and 662 KB
  with every zone doubled in width.

## Tooling

- Data changes: EML (Brabb3l/kfc-parser, v0.2.0, Lua 5.4, GPL-3.0). Patches
  the `.kfc` at process start through a `dbghelp.dll` proxy; data only.
  Known gaps: mod run order is random (PR #21), `dependencies` are not
  enforced, a loader error aborts the process, backup clobbering (PR #22).
  It patches `enshrouded_server.kfc` on disk and keeps a `.kfc.bak`;
  removing `dbghelp.dll` does not undo the patch. A vanilla comparison needs
  a separate, untouched install.
- Runtime hooks: our own proxy-DLL loader in Rust. Ember
  (zachthedev/enshrouded-ember, MIT) has the right shape but no 1.0 build
  support; ShroudForge has no 1.0 server profile; Shroudtopia is dead. Find
  systems by name and fields through the game's reflection data, so hooks
  survive updates better than byte signatures.
- Pure logic (cluster geometry) lives in its own crate with unit and property
  tests, independent of the game.
- CI on every new game build: fetch the dedicated server depot anonymously,
  check that every system, field and signature the mods need still resolves,
  regenerate types, and diff them.

## Environments

- Live server: Linux, Proton, k3s on worker-01, managed from
  HorizonRepublic/hrp-infrastructure. Stays there while it runs well.
- Windows VPS: planned, separate host, for comparing native Windows against
  Proton on the same metrics.
- Windows development machine: OpenSSH over Tailscale for builds and
  automated checks, plus a local agent session for work that needs the
  desktop.
- Test server: a separate instance on a copy of the world, own port and
  password. The live world is untouched until a mod passes there.
  Running on the Windows development machine outside the repository, in
  `D:\ensh-test\`: `server\` (EML, query port 15647, a copy of the live
  world), `server-vanilla\` (never modded, port 15657, for comparisons) and
  `live-copy\` (the copied save).

## Roadmap

1. Test server and the key experiment: change one altar's `TriggerShape` on
   the test server and check that an unmodded client draws and accepts the
   new zone.
2. Repository skeleton: loader, hook layer, geometry crate, CI.
3. Cluster model on the server and client, join check, metrics.
4. Playtest in a test world, then the live world after a save backup.
5. Check Keen's EULA before any public release.

## Risks

- Every game update can move functions and fields; the CI gate above has to
  catch it before players update.
- Changing zone sizes can drop a base's water and reassign entities in
  shifted areas; test on copies.
- Loader maintenance: EML's author was inactive April to October 2026.
