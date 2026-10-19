# Changelog

## v0.1.1 (2026-10-19)

The repository is public.

- MIT licence, third-party notices in the release archive, contribution
  and security guides.

## v0.1.0 (2026-10-19)

First release, for Enshrouded 1.0 build `1269579`.

- Loader: a proxy `POWRPROF.dll` for the dedicated server and the client,
  on Windows and under Proton. It identifies the build by its debug
  fingerprint and runs vanilla on any build it does not know.
- Flame altar zones: touching or overlapping altars form a cluster whose
  area is the sum of their zones; placing or upgrading an altar never
  shrinks a zone.
- Building across the seams between the altars of one cluster.
