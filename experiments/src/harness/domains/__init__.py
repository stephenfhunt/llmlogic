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

from harness.task import Task

#: Every pack in the slate. Order is presentation order in the report.
SLATE = (
    "access_control",
    "ontology",
    "imports",
    "eligibility",
    "scheduling",
    "static_analysis",
    "controls",
)


def available() -> list[str]:
    """Packs that actually exist yet, in slate order."""
    present = {module.name for module in pkgutil.iter_modules(__path__)}
    return [name for name in SLATE if name in present]


def load(name: str) -> list[Task]:
    module = importlib.import_module(f"{__name__}.{name}.tasks")
    return list(module.tasks())


def load_all(names: list[str] | None = None) -> list[Task]:
    return [task for name in (names or available()) for task in load(name)]
