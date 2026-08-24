"""Cutting one named block out of the engine arm's documentation.

**Why this exists.** The skill's guidance accretes: every trap anyone hit becomes
a paragraph, and nothing ever removes one, because nobody can show a line does
*not* carry weight. An ablation is how a doc line is shown to earn its place
rather than asserted to — cut it from one cell's workspace, re-run, and read the
difference.

A block is marked in the skill source with an HTML comment:

    <!-- block: count-wildcard -->
    …the guidance…
    <!-- /block -->

Markers rather than line numbers, because line numbers rot on the first edit and
a rotted ablation cuts the wrong paragraph and reports a result anyway.

**The markers never reach a subject.** `strip` removes them from every copy that
lands in a workspace, ablated or not, so the two conditions differ by the cut
alone and not by a comment the model can see.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

#: `<!-- block: name -->` … `<!-- /block -->`, non-greedy, across lines.
BLOCK = re.compile(
    r"[ \t]*<!--\s*block:\s*(?P<name>[a-z0-9-]+)\s*-->[ \t]*\n"
    r"(?P<body>.*?)"
    r"[ \t]*<!--\s*/block\s*-->[ \t]*\n?",
    re.DOTALL,
)

#: Any marker at all, for the strip that every workspace gets.
MARKER = re.compile(r"[ \t]*<!--\s*/?block(?::\s*[a-z0-9-]+)?\s*-->[ \t]*\n?")

#: Files an ablation may cut from — the skill as a subject receives it.
DOC_SUFFIXES = (".md",)


class UnknownBlock(Exception):
    """An ablation named a block that no document defines."""


@dataclass(frozen=True)
class Block:
    name: str
    #: Path relative to the skill root, e.g. `recipes/source-analysis.md`.
    document: str
    lines: int
    words: int


def blocks_in(text: str, document: str) -> list[Block]:
    found = []
    for match in BLOCK.finditer(text):
        body = match.group("body")
        found.append(
            Block(
                name=match.group("name"),
                document=document,
                lines=len(body.splitlines()),
                words=len(body.split()),
            )
        )
    return found


def catalogue(skill_root: Path) -> dict[str, Block]:
    """Every block the skill defines, by name.

    A duplicate name is an error rather than a merge: two blocks under one name
    means an ablation cuts both and the result names neither.
    """
    found: dict[str, Block] = {}
    for path in sorted(skill_root.rglob("*")):
        if path.suffix not in DOC_SUFFIXES or not path.is_file():
            continue
        document = path.relative_to(skill_root).as_posix()
        for block in blocks_in(path.read_text(encoding="utf-8"), document):
            if block.name in found:
                raise UnknownBlock(
                    f"block {block.name!r} is defined twice — "
                    f"{found[block.name].document} and {document}"
                )
            found[block.name] = block
    return found


def strip(text: str) -> str:
    """Remove every marker, keeping the text between them. What a subject sees."""
    return MARKER.sub("", text)


def cut(text: str, name: str) -> tuple[str, bool]:
    """Remove one named block's body along with its markers.

    Returns the text and whether the block was there, so a caller can tell "cut
    nothing because the block is elsewhere" from "cut nothing because the name is
    wrong" — the second is a broken ablation and the first is not.
    """
    removed = False

    def replace(match: re.Match[str]) -> str:
        nonlocal removed
        if match.group("name") != name:
            return match.group(0)
        removed = True
        return ""

    return BLOCK.sub(replace, text), removed


def apply(skill_root: Path, name: str | None) -> None:
    """Rewrite a workspace's skill copy in place: strip markers, cut one block.

    Called on the **copy** under the workspace, never on the checkout. With
    ``name`` of ``None`` this is the plain strip every engine-arm cell gets.
    """
    documents = [
        path
        for path in sorted(skill_root.rglob("*"))
        if path.suffix in DOC_SUFFIXES and path.is_file()
    ]
    removed = False
    for path in documents:
        text = path.read_text(encoding="utf-8")
        if name is not None:
            text, hit = cut(text, name)
            removed = removed or hit
        path.write_text(strip(text), encoding="utf-8")
    if name is not None and not removed:
        raise UnknownBlock(
            f"no block named {name!r} in {skill_root} — an ablation that cuts "
            "nothing produces a result about nothing"
        )
