# Flame altar zones

Needs the mod on the server and on every player's game.

## In vanilla

Each Flame Altar has a square zone you can build in, and where the zones of
neighbouring altars overlap, the shared part counts only once. Upgrading an
altar pushes its zone further into the neighbour's and loses more.

Where two zones meet, an object has to fit entirely inside one of them, so a
wall across the line between two altars will not go down.

## With the mod

Altars whose zones touch or overlap form one cluster:

- **No area is lost.** The cluster's area is the sum of its altars' zones. What
  the overlap took comes back as a band of equal width around the whole cluster.
- **Build across the seams.** An object anywhere inside the cluster can be
  placed, also across the line between two altars.
- **Zones never shrink** when you place another altar or upgrade one, so nothing
  you built falls out of the base.
- **Height** of every altar in a cluster matches the tallest one.

The order you placed the altars in does not matter; only where they are and
their levels do.

### Example

In a test world, altars of levels 1 and 2 placed with overlapping zones formed
one cluster whose outline moved out by 3.5 m on every side.

### The shape

Every altar keeps its own square zone, grown by the same amount, and the cluster
is all of them together. Altars of different sizes in different places give a
stepped outline, not one rectangle. Altars of one level in a straight row give a
straight edge.

## Log lines

- Server: `altar zones on`, then a `solved <n> altars` line whenever altars are
  placed, upgraded or removed.
- Server and players: `building across seams on`.
