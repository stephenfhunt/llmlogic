"""The corpus, as the subject sees it: a directory of Python files.

No schemas. There is no fact table to declare — inventing one is the task, and a
catalogue of relations would be the extraction already done.
"""

from __future__ import annotations

from harness.corpus import SQLPARSE
from harness.task import Fixture

#: The package root inside the workspace, as the prompt names it.
PACKAGE = SQLPARSE.name


def sources() -> dict[str, str]:
    return SQLPARSE.sources()


def module_name(path: str) -> str:
    """`sqlparse/engine/grouping.py` → `sqlparse.engine.grouping`.

    A package's `__init__.py` is the package itself, which is what makes
    `from sqlparse import engine` an edge to `sqlparse.engine` rather than to a
    file that does not exist.
    """
    return path[: -len(".py")].replace("/", ".").removesuffix(".__init__")


def build() -> Fixture:
    return Fixture(files=dict(sources()), schemas={})
