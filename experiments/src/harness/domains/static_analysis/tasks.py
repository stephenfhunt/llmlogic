"""Four questions over the source tree."""

from __future__ import annotations

from harness.domains.static_analysis import fixture, truth
from harness.task import Task

DOMAIN = "static_analysis"

#: The abstractions, spelled out. This is the domain's central design decision:
#: the questions are *syntactic*, so that both arms and the answer key mean the
#: same thing by "called" and "imports". Real name resolution is ambiguous, and a
#: question that leaned on it would be graded against a guess.
MODULES = (
    f"The package is the `{fixture.PACKAGE}/` directory. A module is one `.py` "
    "file, named by its path with `/` replaced by `.` and `.py` dropped; a "
    f"directory's `__init__.py` is the directory itself, so "
    f"`{fixture.PACKAGE}/engine/__init__.py` is the module "
    f"`{fixture.PACKAGE}.engine`."
)
CALLS = (
    "A call is a call expression, and the name called is the last name before the "
    "parentheses: in `f(x)` it is `f`, and in `obj.f(x)` it is also `f`. Ignore "
    "anything else that is called."
)
TOP_LEVEL = (
    "A module-level function is a `def` at the top level of a file — not inside a "
    "class, and not inside another function."
)


def tasks() -> list[Task]:
    fx = fixture.build()
    common = {"domain": DOMAIN, "fixture": fx}
    return [
        Task(
            id="never-called-functions",
            question=(
                "Which module-level functions defined in the package are never "
                "called anywhere in it? Report the function name. "
                f"{MODULES} {TOP_LEVEL} {CALLS}"
            ),
            truth=truth.never_called_functions(),
            question_class="negation",
            answer_shape=("function",),
            notes=(
                "Every file has to be read for definitions and again for calls, "
                "and the answer is a set difference over both. A `grep` for one "
                "name at a time answers it too — slowly, and only if nothing is "
                "skipped, which is the comparison this domain exists to make."
            ),
            **common,
        ),
        Task(
            id="modules-outside-any-import-cycle",
            question=(
                "Which modules in the package are not part of any import cycle? A "
                "module A imports a module B when A contains `import B`, or "
                "`from B import ...`, or `from P import B` where `P.B` is a module "
                "in the package; relative imports resolve the same way. A module is "
                "in a cycle when it can reach itself by following imports, in one "
                f"step or several. {MODULES}"
            ),
            truth=truth.modules_outside_any_cycle(),
            question_class="recursion",
            answer_shape=("module",),
            notes=(
                "Reachability and then a negation over it. Most of this package is "
                "one big cycle, so the answer is short and an arm that stops at "
                "direct imports gets a much longer one."
            ),
            **common,
        ),
        Task(
            id="token-subclasses",
            question=(
                "Which classes in the package are subclasses of `Token`, directly or "
                "through other classes? A class is a subclass of the names in its "
                "base list, with any qualifier dropped — `class A(mod.B)` makes `A` "
                f"a subclass of `B`. Do not include `Token` itself. {MODULES}"
            ),
            truth=truth.subclasses_of("Token"),
            question_class="recursion",
            answer_shape=("class",),
            notes=(
                "`Token` has one direct subclass and twenty indirect ones, so the "
                "difference between a level and a closure is the whole answer."
            ),
            **common,
        ),
        Task(
            id="functions-called-from-several-modules",
            question=(
                "Which module-level functions defined in the package are called from "
                f"at least {truth.WIDELY_USED_MODULES} different modules? Count the "
                "modules a call appears in, not the number of calls. "
                f"{MODULES} {TOP_LEVEL} {CALLS}"
            ),
            truth=truth.functions_called_from_several_modules(),
            question_class="aggregation",
            answer_shape=("function",),
            notes=(
                "A count of *distinct* modules — two calls in one file are one "
                "module, which is the detail that separates a real count from a "
                "tally of matches."
            ),
            **common,
        ),
    ]
