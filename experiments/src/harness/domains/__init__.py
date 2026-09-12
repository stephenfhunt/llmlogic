"""The domain packs.

Each pack is three files, and the split is not cosmetic:

- ``fixture.py`` — the fact base and its schemas.
- ``truth.py`` — the answers, computed in **plain Python**. Never by the datalog
  engine: if the engine grades itself, the engine arm is correct by construction
  and the run is void while still producing plausible numbers (control 1, and
  ``tests/test_controls.py`` enforces it).
- ``tasks.py`` — the questions, wiring the two together.

Domains vary the *question class* and the *origin of the fact base*, not the
topic. ``controls`` is the odd one out on purpose: it is where the engine is
expected **not** to help, and it is what makes a null result readable rather than
indistinguishable from a broken instrument.
"""

from __future__ import annotations

import importlib
import pkgutil

from harness import generate as generate_module
from harness.task import Task

#: Every pack in the slate. Order is presentation order in the report.
SLATE = (
    "access_control",
    "ontology",
    "imports",
    "eligibility",
    "scheduling",
    "static_analysis",
    "provenance",
    "controls",
)


def _installed() -> set[str]:
    return {module.name for module in pkgutil.iter_modules(__path__)}


def blocked() -> dict[str, str]:
    """Packs that exist but cannot run yet, and why.

    A pack states its own prerequisite by exposing ``unavailable()`` — the one
    case today is ``static_analysis``, whose corpus is fetched rather than
    vendored. Reporting the reason rather than dropping the pack silently is the
    difference between a grid that is smaller than it looks and a grid that says
    so.
    """
    reasons = {}
    for name in sorted(_installed()):
        module = importlib.import_module(f"{__name__}.{name}")
        check = getattr(module, "unavailable", None)
        if check and (reason := check()):
            reasons[name] = reason
    return reasons


def unslated() -> list[str]:
    """Packs that are installed but not in `SLATE`, alphabetically.

    A pack under construction — its fixture built, its questions not — is
    importable and must not be drawn by a grid. Naming it is what keeps *not
    drawn* distinguishable from *not there*.
    """
    return sorted(_installed() - set(SLATE))


def available() -> list[str]:
    """Packs that exist *and* can run, in slate order."""
    present = _installed() - set(blocked())
    return [name for name in SLATE if name in present]


#: Packs with no ``generate``, and why. Stated rather than discovered at call
#: time: a calibrated slate that is quietly missing a domain is the 2026-08-24
#: failure mode wearing a different hat.
NO_GENERATOR = {
    "static_analysis": (
        "its fixture is a fetched, pinned copy of sqlparse — there is nothing to "
        "seed, and a synthetic package would trade away the one property the pack "
        "exists for: real third-party code"
    ),
}


def generators() -> list[str]:
    """Packs that can generate a fresh slate, in slate order.

    Presence of ``generated``, not a list to keep in step with the packs. A pack
    that is deliberately without one says so in `NO_GENERATOR`; a pack that
    simply has not got one yet is absent, and the difference is visible.
    """
    found = []
    for name in available():
        module = importlib.import_module(f"{__name__}.{name}.tasks")
        if hasattr(module, "generated"):
            found.append(name)
    return found


def generate(name: str, seed: int, difficulty: int = 3, track: str = "in-context") -> list[Task]:
    """A fresh slate from one pack. The single entry point a calibration pass
    consumes, so no caller has to know which module a generator lives in."""
    module = importlib.import_module(f"{__name__}.{name}.tasks")
    if not hasattr(module, "generated"):
        reason = NO_GENERATOR.get(name, "not built yet")
        raise ValueError(f"{name} has no generator: {reason}")
    return list(module.generated(seed, difficulty, track))


def validate(name: str, task: Task) -> None:
    """The universal degeneracy rules, then the pack's own invariants.

    Two homes on purpose. `generate.validate` holds what is true of every
    generated item whatever domain it came from; a pack's ``check`` holds what
    only that pack can know — that `eligibility`'s undetermined set is not merely
    its blank-income set, that `scheduling`'s roster obeys the rule its own
    questions state. A pack with nothing to add defines no ``check``.
    """
    generate_module.validate(task)
    module = importlib.import_module(f"{__name__}.{name}.tasks")
    if check := getattr(module, "check", None):
        check(task)


def load(name: str) -> list[Task]:
    module = importlib.import_module(f"{__name__}.{name}.tasks")
    return list(module.tasks())


def load_all(names: list[str] | None = None) -> list[Task]:
    return [task for name in (names or available()) for task in load(name)]
