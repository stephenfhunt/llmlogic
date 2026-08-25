"""The harness refuses an engine binary that is not this checkout's.

Every number the harness produces is a number about whatever was last compiled.
The binary is not versioned and the things measured against it are: the reference
corpus pins live in git, so checking out an earlier commit reddens four of them
until someone remembers to rebuild — and, the same coupling running the other
way, a corpus run right after a source edit reports green against yesterday's
engine. Checking that the file *exists* cannot tell those apart. Checking its
mtime against the sources can.

The guard is deliberately not a build: building here would make the coupling
invisible again, and a release build is a minute the caller should choose to
spend.
"""

import pytest

from harness.arms import EngineMissing, EngineStale, require_engine


def build_tree(root, binary_age=0.0, source_age=10.0):
    """Lay out a stand-in `datalog/` — sources, a `target/release/datalog`.

    Ages are seconds *before now*, so the defaults give a binary newer than
    everything it was built from, which is the fresh case.
    """
    import os
    import time

    now = time.time()

    source_dir = root / "src"
    source_dir.mkdir(parents=True)
    for name in ("lib.rs", "error.rs"):
        path = source_dir / name
        path.write_text("// a source file\n", encoding="utf-8")
        os.utime(path, (now - source_age, now - source_age))
    nested = source_dir / "engine"
    nested.mkdir()
    nested_file = nested / "mod.rs"
    nested_file.write_text("// a nested source file\n", encoding="utf-8")
    os.utime(nested_file, (now - source_age, now - source_age))

    for name in ("Cargo.toml", "Cargo.lock"):
        path = root / name
        path.write_text("# a manifest\n", encoding="utf-8")
        os.utime(path, (now - source_age, now - source_age))

    bin_dir = root / "target" / "release"
    bin_dir.mkdir(parents=True)
    binary = bin_dir / "datalog"
    binary.write_text("#!/bin/sh\n", encoding="utf-8")
    os.utime(binary, (now - binary_age, now - binary_age))

    return bin_dir, binary


def test_a_binary_newer_than_its_sources_is_accepted(tmp_path):
    bin_dir, binary = build_tree(tmp_path)

    assert require_engine(bin_dir=bin_dir, source_root=tmp_path) == binary


def test_no_binary_at_all_is_missing_not_stale(tmp_path):
    bin_dir, binary = build_tree(tmp_path)
    binary.unlink()

    with pytest.raises(EngineMissing) as raised:
        require_engine(bin_dir=bin_dir, source_root=tmp_path)

    assert not isinstance(raised.value, EngineStale)
    assert "cargo build --release --offline" in str(raised.value)


@pytest.mark.parametrize(
    "changed", ["src/error.rs", "src/engine/mod.rs", "Cargo.toml", "Cargo.lock"]
)
def test_a_source_newer_than_the_binary_is_refused(tmp_path, changed):
    """Each build input, one at a time — a nested source counts like a top one."""
    import os
    import time

    bin_dir, _ = build_tree(tmp_path)
    touched = tmp_path / changed
    now = time.time()
    os.utime(touched, (now, now))

    with pytest.raises(EngineStale) as raised:
        require_engine(bin_dir=bin_dir, source_root=tmp_path)

    message = str(raised.value)
    assert changed in message
    assert "cargo build --release --offline" in message


def test_a_file_that_is_not_a_build_input_does_not_make_it_stale(tmp_path):
    """The other half of the guard, and the half that decides if it is usable.

    `spec.md`, `notes/`, `bugs/` and the crate's own `tests/` all change without
    changing the binary. A guard that fired on those would be retrained around
    within a week, which is how a tripwire stops being read.
    """
    import os
    import time

    bin_dir, binary = build_tree(tmp_path)
    now = time.time()
    for name in ("spec.md", "ROADMAP.md"):
        path = tmp_path / name
        path.write_text("# a document\n", encoding="utf-8")
        os.utime(path, (now, now))
    crate_tests = tmp_path / "tests"
    crate_tests.mkdir()
    crate_test = crate_tests / "system.rs"
    crate_test.write_text("// an integration test\n", encoding="utf-8")
    os.utime(crate_test, (now, now))

    assert require_engine(bin_dir=bin_dir, source_root=tmp_path) == binary
