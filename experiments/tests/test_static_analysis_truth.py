"""The `static_analysis` oracle, against second formulations and the real corpus.

This is the one domain whose answer key is computed from someone else's code, so
two different things need checking: that the graph algorithms are right, and that
the pinned corpus still contains the cases the questions are about.
"""

import ast

import pytest
from hypothesis import given
from hypothesis import strategies as st

from harness.corpus import SQLPARSE
from harness.domains.static_analysis import fixture, truth

pytestmark = pytest.mark.skipif(
    not SQLPARSE.present(),
    reason=f"{SQLPARSE.slug} not fetched; run `harness corpus fetch`",
)

_NODE = st.sampled_from(["a", "b", "c", "d", "e"])
_GRAPH = st.dictionaries(keys=_NODE, values=st.sets(_NODE, max_size=3), max_size=5).map(
    lambda edges: {node: set(targets) for node, targets in edges.items()}
)


def reaches_by_fixpoint(edges, start):
    """The same reachability by iterating to a fixpoint instead of by worklist."""
    seen = set(edges.get(start, ()))
    changed = True
    while changed:
        changed = False
        for node in list(seen):
            for target in edges.get(node, ()):
                if target not in seen:
                    seen.add(target)
                    changed = True
    return seen


def cyclic_by_tarjan(edges):
    """Cycle membership from strongly connected components — a different algorithm
    entirely, not a rearrangement of the reachability one."""
    index, low, stack, on_stack, counter, cyclic = {}, {}, [], set(), [0], set()

    def visit(node):
        index[node] = low[node] = counter[0]
        counter[0] += 1
        stack.append(node)
        on_stack.add(node)
        for target in edges.get(node, ()):
            if target not in index:
                visit(target)
                low[node] = min(low[node], low[target])
            elif target in on_stack:
                low[node] = min(low[node], index[target])
        if low[node] == index[node]:
            component = []
            while True:
                current = stack.pop()
                on_stack.discard(current)
                component.append(current)
                if current == node:
                    break
            if len(component) > 1 or node in edges.get(node, ()):
                cyclic.update(component)

    for node in edges:
        if node not in index:
            visit(node)
    return cyclic


@given(edges=_GRAPH, start=_NODE)
def test_reachability_agrees_with_an_independent_formulation(edges, start):
    assert truth.reaches(edges, start) == reaches_by_fixpoint(edges, start)


@given(edges=_GRAPH)
def test_cycle_membership_agrees_with_strongly_connected_components(edges):
    complete = {node: edges.get(node, set()) for node in edges}
    mine = {node for node in complete if node in truth.reaches(complete, node)}
    assert mine == cyclic_by_tarjan(complete)


def test_the_corpus_parses_at_all():
    # The answer key is computed by parsing; a file that does not parse would
    # take its definitions and calls out of every answer with nothing said.
    for path, text in fixture.sources().items():
        ast.parse(text, filename=path)


def test_module_names_follow_the_stated_rule():
    assert fixture.module_name("sqlparse/engine/grouping.py") == "sqlparse.engine.grouping"
    assert fixture.module_name("sqlparse/engine/__init__.py") == "sqlparse.engine"
    assert fixture.module_name("sqlparse/__init__.py") == "sqlparse"


def test_every_import_edge_lands_on_a_module_that_exists():
    modules = set(truth.import_edges())
    for module, targets in truth.import_edges().items():
        assert targets <= modules, module
        assert module not in targets, f"{module} imports itself"


def test_the_package_really_does_contain_a_cycle_and_something_outside_it():
    cyclic = truth.modules_in_a_cycle()
    outside = {module for (module,) in truth.modules_outside_any_cycle().rows}
    assert cyclic and outside
    assert not (cyclic & outside)
    assert cyclic | outside == set(truth.import_edges())


def test_a_module_outside_a_cycle_cannot_reach_itself():
    edges = truth.import_edges()
    for (module,) in truth.modules_outside_any_cycle().rows:
        assert module not in truth.reaches(edges, module)


def test_never_called_functions_really_appear_nowhere_as_a_callee():
    calls = truth.calls_by_module()
    never = {name for (name,) in truth.never_called_functions().rows}
    assert never
    for name in never:
        assert not calls[name]
    # …and the complement is not empty either: a corpus where nothing is called
    # would pass the check above while meaning the extraction is broken.
    assert truth.module_level_functions() - never


def test_a_function_is_counted_once_per_module_however_often_it_is_called():
    calls = truth.calls_by_module()
    widely = {name for (name,) in truth.functions_called_from_several_modules().rows}
    assert widely
    for name in widely:
        assert len(calls[name]) >= truth.WIDELY_USED_MODULES
    for name in truth.module_level_functions() - widely:
        assert len(calls[name]) < truth.WIDELY_USED_MODULES


def test_the_subclass_answer_is_a_closure_and_not_a_level():
    below = {name for (name,) in truth.subclasses_of("Token").rows}
    bases = truth.base_names()
    direct = {name for name, parents in bases.items() if "Token" in parents}
    assert direct
    assert direct < below, "the closure adds nothing, so the question tests nothing"
    assert "Token" not in below
    for name in below:
        assert bases[name] & (below | {"Token"})


def test_a_class_outside_the_hierarchy_is_not_included():
    below = {name for (name,) in truth.subclasses_of("Token").rows}
    everything = set(truth.base_names())
    assert everything - below, "every class descends from Token; the answer is the universe"
