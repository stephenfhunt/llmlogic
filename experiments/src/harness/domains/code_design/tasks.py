"""Design questions over a codebase nobody can read in one sitting.

**The abstractions are spelled out, and they are syntactic.** That is this
pack's central decision, inherited from `static_analysis`: the questions are
about shapes a reader can compute the same way the answer key does, so both arms
and the key mean one thing by "imports". Real name resolution and TypeScript's
import elision are *semantic*, and a question that leaned on either would be
graded against a guess — measured here, not assumed: TypeScript drops 19% of
`vs/base`'s import statements because their bindings are only used as types, and
no rule a reader can apply sees that.

Three templates, each parametrized by directory. What is **not** here is as
deliberate: class and module cohesion were built, checked against an independent
implementation, and dropped when 47 of 187 classes disagreed with no single
cause (`../../../decisions.md` 2026-09-11 later iv). The import graph survives
that check exactly — 2,039 edges, no disagreement — and is the whole of what
these questions rest on.
"""

from __future__ import annotations

from harness.domains.code_design import fixture, truth
from harness.task import Task

DOMAIN = "code_design"

#: Where the corpus sits inside the workspace, as every question names it.
ROOT = f"{fixture.PACKAGE}/"

#: The test tree, for the template that asks what no test names.
TEST_ROOT = "src/vs/base/test"

#: The band an answer has to land in to be an item.
#:
#: **Above**, because an item whose answer is 800 rows is one both arms always
#: miss, and a pair that is always concordant tells McNemar nothing about the
#: difference between them. **Below**, because `generate.validate` rejects a
#: single-row answer as close to guessable — a universal rule this pack does not
#: get to opt out of, and the reason the floor is 2 rather than 1.
BAND = range(2, 21)

FILES = (
    f"A file is a `.ts` file under `{ROOT}`, named by its path with the "
    f"`{fixture.PACKAGE}/` prefix dropped — so "
    "`src/vs/base/common/uri.ts`. A `.d.ts` file is not one: it declares types "
    "and has no code."
)

IMPORTS = (
    "File A **imports** file B when A contains an `import` declaration, a "
    "re-export (`export … from`), or an `import(...)` expression whose "
    "specifier resolves to B. A specifier is relative (it starts with `.`) and "
    "resolves by path, with TypeScript's `.js` suffix meaning the `.ts` file "
    "beside it; a specifier that does not start with `.` names a package, not a "
    "file here. **Type-only imports count** — `import type`, and a name written "
    "`type X` — because this question is about what the source says, not about "
    "what the compiler emits."
)

EXPORTS = (
    "An **exported declaration** of a file is a top-level `export`ed "
    "`function`, `class`, `interface`, `type`, `enum` or `const`/`let`/`var`, "
    "named by the name it binds. A file's `export … from` re-export is not one "
    "of its own declarations."
)

DIRECTORY = (
    "A directory includes everything beneath it, so the files of "
    "`src/vs/base/parts` include `src/vs/base/parts/ipc/node/ipc.cp.ts`."
)


def _cycle(directory: str) -> Task:
    return Task(
        domain=DOMAIN,
        id=f"import-cycle-{directory.removeprefix('src/vs/').replace('/', '-')}",
        question=(
            f"Which files in `{directory}` are part of an import cycle made "
            f"only of files in `{directory}`? A file is in such a cycle when it "
            "can reach itself by following imports, in one step or several, "
            f"without leaving `{directory}`. Report the file path. "
            f"{FILES} {DIRECTORY} {IMPORTS}"
        ),
        truth=truth.files_in_runtime_cycle(directory),
        fixture=fixture.build(),
        question_class="recursion",
        answer_shape=("file",),
        track="at-scale",
        notes=(
            "Reachability over a graph the subject has to build first, from "
            "hundreds of files it cannot all read. Scoped to one directory so "
            "the answer is checkable and the template yields more than one item."
        ),
    )


def _unimported(directory: str) -> Task:
    return Task(
        domain=DOMAIN,
        id=f"unimported-{directory.removeprefix('src/vs/').replace('/', '-')}",
        question=(
            f"Which exported declarations of the files in `{directory}` does no "
            "**other** file in the corpus import by name? Report `file|name`. "
            "Count only direct imports: if file X imports a name from file Y and "
            "Y re-exports it from Z, that names Y, not Z. An "
            "`import * as ns` from a file counts as importing every declaration "
            f"that file exports. {FILES} {DIRECTORY} {EXPORTS} {IMPORTS}"
        ),
        truth=truth.unimported_exports(directory),
        fixture=fixture.build(),
        question_class="negation",
        answer_shape=("file", "name"),
        track="at-scale",
        notes=(
            "A set difference over every import in the corpus against every "
            "export in one directory — the shape that punishes sampling, since "
            "one missed importer turns a used export into a dead one."
        ),
    )


def _untested(directory: str) -> Task:
    return Task(
        domain=DOMAIN,
        id=f"untested-{directory.removeprefix('src/vs/').replace('/', '-')}",
        question=(
            f"Which exported declarations of the files in `{directory}` does no "
            f"file under `{TEST_ROOT}` import by name? Report `file|name`. Only "
            f"what a test file *names* counts, not what it exercises through "
            f"another module. Files under `{TEST_ROOT}` are not themselves part "
            f"of the answer. {FILES} {DIRECTORY} {EXPORTS} {IMPORTS}"
        ),
        truth=truth.exports_no_test_imports(directory, TEST_ROOT),
        fixture=fixture.build(),
        question_class="negation",
        answer_shape=("file", "name"),
        track="at-scale",
        notes=(
            "The same negation against a different universe. Its blind spot is "
            "stated in the question — a name reached indirectly counts as "
            "untested here — because 'what a test covers' is a question about "
            "running it, and no static answer to that is checkable."
        ),
    )


def _all_files(directory: str) -> set[tuple[str, ...]]:
    return {(path,) for path in truth.files_under(directory)}


def _all_exports(directory: str) -> set[tuple[str, ...]]:
    return {
        (path, declaration["name"])
        for path in truth.files_under(directory)
        for declaration in truth._exports_of(path)
    }


#: Each template: how to build it, the answer it is sized by, and the universe
#: that answer is drawn from. The universe is what makes "the answer is
#: everything" checkable — `generate.validate` enforces that rule only over
#: `.csv` fixtures, and this pack has none, so the rule has to be applied here
#: or not at all. It bites: every file of `browser/ui/selectBox` is in an import
#: cycle, and that item would have been passed by listing the directory.
TEMPLATES = (
    (_cycle, truth.files_in_runtime_cycle, _all_files),
    (_unimported, truth.unimported_exports, _all_exports),
    (_untested, lambda d: truth.exports_no_test_imports(d, TEST_ROOT), _all_exports),
)


def tasks() -> list[Task]:
    """Every template over every directory whose answer lands in `BAND`.

    Deduplicated by answer: a directory and its only populated child produce the
    same rows, and asking one question twice is the failure the pre-registration
    named. Dropped when the answer *is* the universe, which scores correct by
    copying a listing. Selection is by the answer's **size and degeneracy**,
    never by its content.
    """
    found: list[Task] = []
    for build, answer_of, universe_of in TEMPLATES:
        seen: set[frozenset] = set()
        for directory in truth.directories(minimum=3):
            answer = answer_of(directory)
            if len(answer) not in BAND or answer.rows in seen:
                continue
            if set(answer.rows) == universe_of(directory):
                continue
            seen.add(answer.rows)
            found.append(build(directory))
    return found


def check(task: Task) -> None:
    """This pack's own invariants, beyond the universal degeneracy rules."""
    assert task.track == "at-scale", task.key
    assert len(task.truth) in BAND, f"{task.key}: {len(task.truth)} rows"
