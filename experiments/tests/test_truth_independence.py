"""Control 1: ground truth is never computed by the engine being measured.

If a domain's ``truth.py`` reaches for the datalog binary, the engine arm is
correct by construction and the whole run is void — while still producing a full
grid of plausible numbers. That failure is silent, which is why it gets a test
instead of a convention.
"""

import ast
from pathlib import Path

import harness

DOMAINS_DIR = Path(harness.__file__).resolve().parent / "domains"

FORBIDDEN_IMPORTS = {"subprocess", "os", "shutil", "harness.arms", "harness.subject"}


def _executable_strings(tree: ast.AST) -> list[str]:
    """String literals excluding docstrings.

    Checked over the AST rather than raw text so a *comment* saying "never shell
    out to datalog" passes while code that actually names the binary does not.
    """
    docstrings = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Module | ast.ClassDef | ast.FunctionDef | ast.AsyncFunctionDef):
            body = getattr(node, "body", [])
            if (
                body
                and isinstance(body[0], ast.Expr)
                and isinstance(body[0].value, ast.Constant)
                and isinstance(body[0].value.value, str)
            ):
                docstrings.add(id(body[0].value))
    return [
        node.value
        for node in ast.walk(tree)
        if isinstance(node, ast.Constant)
        and isinstance(node.value, str)
        and id(node) not in docstrings
    ]


def _forbidden(module: str) -> bool:
    """Match the dotted name and its root — ``harness.arms`` and ``subprocess.run``
    are both ways in, and checking only the root missed the first one."""
    return module in FORBIDDEN_IMPORTS or module.split(".")[0] in FORBIDDEN_IMPORTS


def engine_violations(tree: ast.AST, label: str) -> list[str]:
    """Ways a ``truth.py`` could be letting the engine compute its own grade."""
    found = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if _forbidden(alias.name):
                    found.append(f"{label} imports {alias.name}")
        elif isinstance(node, ast.ImportFrom) and node.module:
            if _forbidden(node.module):
                found.append(f"{label} imports from {node.module}")
    for literal in _executable_strings(tree):
        if "datalog" in literal.lower():
            found.append(f"{label} names datalog in executable code: {literal!r}")
    return found


def test_no_domain_computes_its_truth_with_the_engine():
    checked = 0
    for truth_file in sorted(DOMAINS_DIR.glob("*/truth.py")):
        violations = engine_violations(ast.parse(truth_file.read_text()), str(truth_file))
        checked += 1
        assert not violations, violations
    assert checked > 0, "no truth.py files found — the check is silently passing"


def test_the_check_catches_a_truth_file_that_shells_out():
    # A guard that cannot fail is worse than no guard: it reads like coverage.
    source = (
        '"""Truth."""\n'
        "import subprocess\n"
        "\n"
        "def answers():\n"
        '    return subprocess.run(["datalog", "q.dl"])\n'
    )
    assert engine_violations(ast.parse(source), "shells-out")


def test_the_check_catches_a_truth_file_that_names_the_binary():
    source = '"""Truth."""\n\ndef answers():\n    return run_tool("datalog")\n'
    assert engine_violations(ast.parse(source), "names-binary")


def test_the_check_catches_a_truth_file_that_borrows_the_workspace():
    source = '"""Truth."""\nfrom harness.arms import build\n\ndef answers():\n    return build\n'
    assert engine_violations(ast.parse(source), "borrows-workspace")


def test_the_check_passes_an_honest_truth_file_including_its_comments():
    source = (
        '"""Truth, computed in plain Python. Never the datalog engine."""\n'
        "\n"
        "# Deliberately no shelling out to datalog here.\n"
        "def answers():\n"
        "    return [1, 2]\n"
    )
    assert engine_violations(ast.parse(source), "honest") == []
