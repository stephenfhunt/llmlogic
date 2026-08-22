"""The corpus pin, and the guards on unpacking someone else's archive.

``static_analysis``'s answer key is computed from the corpus, so an unpinned or
substituted corpus is a changed answer key with nothing saying so.
"""

import hashlib
import io
import re
import tarfile

import pytest

from harness import corpus


def test_every_corpus_is_pinned_by_version_and_hash():
    for item in corpus.CORPORA:
        assert re.fullmatch(r"[0-9a-f]{64}", item.sha256), item
        # The URL must name the exact version too: a "latest" link would move the
        # tree under a run while the hash check reported the wrong archive rather
        # than the real problem.
        assert item.version in item.url, item


def test_a_corpus_that_is_not_fetched_says_how_to_fetch_it(tmp_path, monkeypatch):
    monkeypatch.setattr(corpus, "CACHE_ROOT", tmp_path)
    absent = corpus.Corpus(
        name="nothing",
        version="1.0",
        sha256="0" * 64,
        url="https://example.invalid/nothing-1.0.tar.gz",
        package_dir="nothing-1.0/nothing",
    )
    with pytest.raises(corpus.CorpusMissing) as caught:
        absent.require()
    assert "harness corpus fetch" in str(caught.value)


def _tarball(members: dict[str, str]) -> bytes:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w:gz") as tar:
        for name, contents in members.items():
            data = contents.encode()
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    return raw.getvalue()


def _served(monkeypatch, archive: bytes) -> None:
    class Response:
        def read(self):
            return archive

    monkeypatch.setattr(corpus.urllib.request, "urlopen", lambda *a, **k: Response())


def _pinned(tmp_path, archive: bytes, monkeypatch) -> corpus.Corpus:
    monkeypatch.setattr(corpus, "CACHE_ROOT", tmp_path)
    return corpus.Corpus(
        name="pkg",
        version="1.0",
        sha256=hashlib.sha256(archive).hexdigest(),
        url="https://example.invalid/pkg-1.0.tar.gz",
        package_dir="pkg-1.0/pkg",
    )


def test_fetch_extracts_only_the_package_directory(tmp_path, monkeypatch):
    archive = _tarball(
        {
            "pkg-1.0/pkg/__init__.py": "x = 1\n",
            "pkg-1.0/pkg/core/engine.py": "def run():\n    return 1\n",
            "pkg-1.0/setup.py": "raise SystemExit\n",
        }
    )
    item = _pinned(tmp_path, archive, monkeypatch)
    _served(monkeypatch, archive)

    corpus.fetch(item)
    assert item.present()
    assert sorted(item.sources()) == ["pkg/__init__.py", "pkg/core/engine.py"]
    assert not (item.root / "setup.py").exists()


def test_a_hash_mismatch_refuses_to_extract(tmp_path, monkeypatch):
    archive = _tarball({"pkg-1.0/pkg/__init__.py": "x = 1\n"})
    item = _pinned(tmp_path, archive, monkeypatch)
    object.__setattr__(item, "sha256", "1" * 64)
    _served(monkeypatch, archive)

    with pytest.raises(ValueError):
        corpus.fetch(item)
    assert not item.present()


def test_a_member_that_climbs_out_of_the_tree_is_skipped(tmp_path, monkeypatch):
    # A pinned tarball is still an archive written by someone else, and the pin
    # says it is unchanged — not that it is harmless.
    archive = _tarball(
        {"pkg-1.0/pkg/__init__.py": "x = 1\n", "pkg-1.0/pkg/../../escaped.py": "boom\n"}
    )
    item = _pinned(tmp_path, archive, monkeypatch)
    _served(monkeypatch, archive)

    corpus.fetch(item)
    assert sorted(item.sources()) == ["pkg/__init__.py"]
    assert not (tmp_path.parent / "escaped.py").exists()
