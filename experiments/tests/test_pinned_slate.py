"""The 31 pinned tasks, hashed. Nothing below may move them.

The generators are a refactor of every pack's fixture into a value the oracle can
be handed — and a refactor that quietly changes a fixture changes what the
comparable slate measures, without changing a task id or a question. That is
exactly the defect `resume.fingerprint` was written for on 2026-08-24, pointed at
the source tree instead of at a resumed run.

One test rather than a copy per pack, because the rule is one rule: *the slate
the 2026-08-24 grid measured is the slate we still have.* A deliberate change to
a pinned task is a one-line edit here, and having to make it is the point.
"""

from harness import domains, resume

#: task key -> sha256 prefix, taken from the tree before the generator work began.
PINNED = {
    "access_control/who-can-read-r03": "0499ee734ce6341f",
    "access_control/resources-for-u04": "bac0dc855085b84e",
    "access_control/users-with-no-access": "7d7ca9fd98a296e1",
    "access_control/delete-without-read": "5a8b0fd34f012c34",
    "ontology/instances-of-sensor": "337d7c1272affa84",
    "ontology/power-profile": "abbb1aac135b4944",
    "ontology/inconsistent-instances": "aaf733d932265105",
    "ontology/classes-with-no-instances": "f28d6e04ffbae8c1",
    "imports/late-shipments": "7d6238ec4fabe573",
    "imports/regions-over-threshold": "46c34ed4ff41088e",
    "imports/customers-with-no-recent-orders": "bd310d78716ad51c",
    "imports/busiest-month-per-region": "ac39ba49c2e16976",
    "eligibility/eligible": "7cc487efd54d1aee",
    "eligibility/blocked-by-exactly-one": "6c5c9cb8d96eafbd",
    "eligibility/undetermined": "1ac7ece85256cf7d",
    "eligibility/households-over-dependants": "3e2f1d975d74acc4",
    "scheduling/double-booked": "889f40368794e480",
    "scheduling/unstaffable-shifts": "4b283a0cc678ba8b",
    "scheduling/forced-assignments": "5c3e5238eb258080",
    "scheduling/rest-violations": "fb3fe6d9037b1a2b",
    "static_analysis/never-called-functions": "43cb8d675b7c602f",
    "static_analysis/modules-outside-any-import-cycle": "108c27e7304da69f",
    "static_analysis/token-subclasses": "70f144d369d930b1",
    "static_analysis/functions-called-from-several-modules": "45ae6410967678e4",
    "provenance/critical-grant": "92ee22ffdefa4182",
    "provenance/minimal-repair": "9c58b90f9705af82",
    "provenance/access-path": "d69ebb8ece425f5a",
    "controls/department-of": "567d471f8808ed9b",
    "controls/customer-of-order": "b3be9a92dea55f77",
    "controls/orders-above-100": "a30f549bcca6c4ce",
    "controls/engineering-headcount": "397c66f0ec307dd6",
}


def test_every_available_pinned_task_hashes_to_what_it_did():
    """`static_analysis` is skipped when its corpus is not fetched, and only
    then: a pack that is merely *unavailable* must not silently drop its pins."""
    current = resume.fingerprints(domains.load_all())
    expected = {key: value for key, value in PINNED.items() if key in current}
    assert current == expected


def test_the_pins_cover_every_pack_in_the_slate():
    """A new pack with no pin is a pack whose fixture can move unnoticed."""
    assert {key.split("/")[0] for key in PINNED} == set(domains.SLATE)
