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

CORPORA: tuple[Corpus, ...] = (SQLPARSE,)


def fetch(corpus: Corpus, force: bool = False) -> Path:
    """Download, verify and extract one corpus. Idempotent."""
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
