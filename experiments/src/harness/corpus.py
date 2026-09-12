"""Source corpora, fetched at setup time and cached outside the checkout.

``static_analysis`` asks questions about a **real** codebase, and a cell is
sealed — no network, nothing outside its workspace — so the tree has to be on
disk before a run starts. It is fetched rather than vendored: a third-party
package copied into this repo would put someone else's licence in a repo whose
own licensing is deliberately unsettled, and it would be a few thousand lines of
code nobody here maintains.

Fetching is only reproducible if it is **pinned**, so a corpus names its version
*and* the sha256 of the archive. An unpinned "latest" would move the answer key
under a run without anything saying so.
"""

from __future__ import annotations

import hashlib
import io
import os
import shutil
import subprocess
import tarfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

#: Beside the workspaces, and for the same reason: outside the checkout. A corpus
#: inside it would be reachable from a cell by climbing two directories.
CACHE_ROOT = Path(
    os.environ.get(
        "HARNESS_CORPUS_ROOT",
        Path.home() / ".cache" / "llmlogic-experiments" / "corpora",
    )
)

#: Committed manifests a corpus is installed from — a lockfile, never the
#: packages it names. Repo-relative, resolved from this file.
MANIFEST_ROOT = Path(__file__).resolve().parents[2] / "corpora"


class CorpusMissing(Exception):
    """A corpus a domain needs has not been fetched yet."""


@dataclass(frozen=True)
class Corpus:
    name: str
    version: str
    #: sha256 of the archive, checked on every fetch. The pin is the point.
    sha256: str
    url: str
    #: Path inside the archive holding the package, relative to its root.
    package_dir: str

    @property
    def slug(self) -> str:
        return f"{self.name}-{self.version}"

    @property
    def root(self) -> Path:
        return CACHE_ROOT / self.slug

    def present(self) -> bool:
        return self.root.is_dir() and any(self.root.rglob("*.py"))

    def require(self) -> Path:
        if not self.present():
            raise CorpusMissing(
                f"{self.slug} is not in {CACHE_ROOT} — fetch it with "
                f"`harness corpus fetch` (it needs the network, which a cell does not have)"
            )
        return self.root

    def sources(self) -> dict[str, str]:
        """Every ``.py`` file in the corpus, keyed by its path within the tree.

        Read once, held as text, and handed to the fixture — so a domain's
        ``truth.py`` parses exactly the bytes the subject was given, and neither
        of them touches the filesystem to disagree about it.
        """
        root = self.require()
        return {
            f"{self.name}/{path.relative_to(root).as_posix()}": path.read_text(encoding="utf-8")
            for path in sorted(root.rglob("*.py"))
        }


@dataclass(frozen=True)
class GitCorpus:
    """A corpus pinned by **commit**, cloned rather than downloaded.

    `Corpus` pins the sha256 of an archive, which is the right pin for an sdist
    on PyPI — those bytes are immutable. A repository has no such archive:
    GitHub generates a tarball per request and promises nothing about its bytes,
    so the same tag can hash two ways and the pin would fail for a reason that
    is not the one it exists to catch. A commit id is what git guarantees, and
    `git rev-parse HEAD` is the check.

    The tag is what is *fetched* (a shallow clone of one ref is cheap); the
    commit is what is *verified*. A moved tag therefore fails loudly instead of
    silently changing the fixture.
    """

    name: str
    version: str
    #: The full 40-character commit id the tag must resolve to.
    commit: str
    #: The git remote.
    url: str

    @property
    def slug(self) -> str:
        return f"{self.name}-{self.version}"

    @property
    def root(self) -> Path:
        return CACHE_ROOT / self.slug

    def present(self) -> bool:
        return (self.root / ".git").is_dir() or (self.root / "package.json").is_file()

    def require(self) -> Path:
        if not self.present():
            raise CorpusMissing(
                f"{self.slug} is not in {CACHE_ROOT} — fetch it with "
                f"`harness corpus fetch` (it needs the network, which a cell does not have)"
            )
        return self.root

    def fetch(self, force: bool = False) -> Path:
        if self.present() and not force:
            return self.root
        if self.root.exists():
            shutil.rmtree(self.root)
        subprocess.run(
            ["git", "clone", "--depth", "1", "--branch", self.version, self.url, str(self.root)],
            check=True,
        )
        head = subprocess.run(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        if head != self.commit:
            shutil.rmtree(self.root)
            raise ValueError(
                f"{self.slug} tag {self.version} is at {head}, expected {self.commit}. "
                "The pin is what makes the fixture reproducible — do not relax it to "
                "make a fetch succeed."
            )
        return self.root


@dataclass(frozen=True)
class NodePackages:
    """Node packages installed from a **committed lockfile**.

    The pin is the lockfile: `npm ci` refuses to resolve anything it does not
    name and checks an integrity hash for every package, transitive ones
    included — which is more than a hand-written list of tarball hashes would
    do, and is the mechanism npm already guarantees.

    Only the manifest is in the repository. No third-party bytes are committed,
    for the reason at the top of this module.
    """

    name: str
    #: The committed directory holding `package.json` and `package-lock.json`.
    manifest: Path

    @property
    def slug(self) -> str:
        return self.name

    @property
    def root(self) -> Path:
        return CACHE_ROOT / self.name

    @property
    def url(self) -> str:
        """What `harness corpus` prints as the source. The registry, by lockfile."""
        return f"npm ci ({self.manifest.name}/package-lock.json)"

    def present(self) -> bool:
        return (self.root / "node_modules").is_dir()

    def require(self) -> Path:
        if not self.present():
            raise CorpusMissing(
                f"{self.name} is not installed in {CACHE_ROOT} — fetch it with "
                f"`harness corpus fetch` (it needs the network, which a cell does not have)"
            )
        return self.root

    def fetch(self, force: bool = False) -> Path:
        if self.present() and not force:
            return self.root
        self.root.mkdir(parents=True, exist_ok=True)
        for name in ("package.json", "package-lock.json"):
            shutil.copy2(self.manifest / name, self.root / name)
        subprocess.run(["npm", "ci", "--no-audit", "--no-fund"], cwd=self.root, check=True)
        return self.root


#: The `static_analysis` corpus. A real parser with a genuinely tangled call
#: graph — token filters, dispatch through a registry — and not so famous that a
#: model can answer from memory instead of from the files, which `requests` is.
SQLPARSE = Corpus(
    name="sqlparse",
    version="0.6.0",
    sha256="113c35c75365ab9cc9c7231d68c6428fb11c085fc8e9eb1ad659b7ddbf6cd2b9",
    url=(
        "https://files.pythonhosted.org/packages/5f/d3/"
        "3f06a1006f2261d1342aefb3c71eed02f5d4ca5bdbecd86ebc12ad38306e/sqlparse-0.6.0.tar.gz"
    ),
    package_dir="sqlparse-0.6.0/sqlparse",
)

#: The `code_design` corpus. Pinned by **commit, not by archive**: GitHub's
#: generated tarballs carry no stability promise — the same tag can produce
#: different bytes — so a sha256 of one would be a pin that quietly stops
#: matching. A commit id is the thing git itself guarantees.
#:
#: The clone is the whole repository; `domains/code_design/fixture.py` selects
#: the subtree the pack ships. Shallow, because 2.87M lines of history is not
#: the fixture.
VSCODE = GitCorpus(
    name="vscode",
    version="1.137.0",
    commit="645f29cc3176500b4b5762ba887cf2a7f0ffdf2c",
    url="https://github.com/microsoft/vscode.git",
)

#: What `vs/base` is compiled against, installed from the lockfile committed at
#: `corpora/vs-base-types/`. Without these the extractor leaves **12,061** names
#: unresolved against 286 — see that directory's README. It changes one
#: cross-file reference edge in 4,475 and no cycle, so it is not the answer key
#: that depends on it; it is the blind spot an engine arm reads off `orient.dl`,
#: and only the engine arms extract at all.
VSCODE_TYPES = NodePackages(name="vs-base-types", manifest=MANIFEST_ROOT / "vs-base-types")

CORPORA: tuple[Corpus | GitCorpus | NodePackages, ...] = (SQLPARSE, VSCODE, VSCODE_TYPES)


def fetch(corpus: Corpus | GitCorpus | NodePackages, force: bool = False) -> Path:
    """Fetch one corpus, however it is pinned. Idempotent.

    The kinds that pin themselves carry their own `fetch`; what is left here is
    the archive-and-hash path, which is the oldest and is spelled out below.
    """
    if not isinstance(corpus, Corpus):
        return corpus.fetch(force)
    if corpus.present() and not force:
        return corpus.root

    archive = urllib.request.urlopen(corpus.url, timeout=60).read()
    digest = hashlib.sha256(archive).hexdigest()
    if digest != corpus.sha256:
        raise ValueError(
            f"{corpus.slug} archive hash is {digest}, expected {corpus.sha256}. "
            "The pin is what makes the fixture reproducible — do not relax it to "
            "make a fetch succeed."
        )

    corpus.root.mkdir(parents=True, exist_ok=True)
    prefix = corpus.package_dir.rstrip("/") + "/"
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
        for member in tar.getmembers():
            if not member.isfile() or not member.name.startswith(prefix):
                continue
            relative = Path(member.name[len(prefix) :])
            if relative.is_absolute() or ".." in relative.parts:
                continue  # a tarball is untrusted input, whatever it is a pin of
            handle = tar.extractfile(member)
            if handle is None:
                continue
            destination = corpus.root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(handle.read())
    return corpus.root
