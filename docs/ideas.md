# Mod ideas

Ideas for the pack, newest first. Each one says where it stands; a mod that
reaches a release moves to the Mods table in the README and gets a page in
`docs/mods/`.

## Personal chests

A chest only its owner can open, with its own crafting recipe, for each
player's own things in a shared base.

Open questions:

- Who owns a chest: whoever crafted it or whoever placed it.
- What admins can do: open it, or only break it.
- Whether a Magic chest of this kind feeds nearby workshops for its owner
  alone.

## Area loot

Loot from every enemy you killed nearby in one go, as in World of Warcraft: pick
up the drops of one and the drops of the others around come with them. The loot
itself stays vanilla. Only collecting it gets quicker.

Open questions:

- How far around the looted enemy the others count.
- Whose loot it is when friends fight together: only what the looting player may
  pick up, as vanilla decides it.
- How Enshrouded drops loot today (items on the ground, a container on the
  body), which decides where the hook goes.

## Shared build camera

A free build camera for every player on the server. A community mod already
offers a build camera; ours comes later, built on the same loader as the rest of
the pack.

## Shared experience

Every player gets the experience any player earns, so friends who play at
different hours stay on the same level.

Open questions:

- Online players only, or also players who are offline and catch up when they
  next join?
- Everyone on the server, or only players within some distance, as with a party?
- Experience only, or also the Flame level progress and skill points that come
  with levels?

## Survivors per altar

A copy of a survivor, such as the Blacksmith, at every altar, so each player's
house has its own. Rules: one copy per altar zone, so a cluster of N altars
holds at most N copies; copies only inside a zone; later a summoning staff as
the price.

State: the experiment on the test server worked. Copies show up on unmodded
clients, and both dialogue and crafting work with each of them. With the
experiment off, the game removes the copies again. Next: copies told apart by
altar, with their own beds.

## Join check enforced by the game

A client without the mods or on another version cannot join, through the game's
own build check, instead of the warning it gets today.

## Self-update for players

A modded client downloads the release the server runs from GitHub, checks its
signature and asks for a restart. Never from the game server itself.

## Building blocks and decor yield

Twice the blocks and decor per craft, instead of half the cost, so an unmodded
display never shows a cost the server will not charge.

## Furniture comfort

More comfort from furniture, so each house reaches the rested bonus.

## Chest consolidation

Items flow between chests on their own. Vanilla 1.0 already names chests, makes
Magic chests feed nearby workshops and has the Quick Stash Station; this goes
further and needs runtime hooks.

## Altars past 16

More than 16 altars in a world, which needs hooks on the cap in the cursor check
and on the 16-zone limit in base grouping.

## Dev tools for test servers

Commands for testing on a test server only: give an item, teleport, set the
Flame level. Comes with the custom summoning staff, which needs a way to hand
it out before it drops in game.

## A loader for every modder

The loader and its API standardised for mods by other authors: an SDK crate,
mods as separate DLLs with a versioned C ABI, hooks shared by more than one mod
in a chain. Planned for the move to the new repository.
