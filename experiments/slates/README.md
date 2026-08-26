# `slates/` — calibrated slates

One manifest per calibration pass: the items `harness calibrate` selected, the
ones it rejected and the rate that rejected them, and the
`(pack, seed, difficulty, track)` each was drawn from.

A manifest holds **provenance and a fingerprint, not fixtures**. `harness run
--slate <manifest>` regenerates each item and refuses one that no longer hashes
to what the pass measured — a generator whose threshold or wording moved would
otherwise hand the grid a slate nobody calibrated.

These are tracked: a pinned slate is part of the experiment's definition, not an
output of it. A `--dry-run` pass writes its manifest inside its own run directory
instead, because a stub's selection is evidence about the plumbing and nothing
else.
