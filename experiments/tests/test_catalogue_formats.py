"""Control 2 over the formats a fixture can actually ship.

The schema check used to know one format. It now has to hold for a relation that
ships as text *and* as Parquet, and for a fixture whose files are a source tree
with no schema at all — and control 2 is only worth anything if it fails loudly
in each of them.
"""

import io

import pyarrow
import pyarrow.parquet as pq
import pytest

from harness.catalogue import SchemaDrift, relation_catalogue, verify
from harness.task import Fixture

ORDER_CSV = "id,amount\no1,40\no2,150\n"


def parquet_bytes(columns: dict[str, list]) -> bytes:
    sink = io.BytesIO()
    pq.write_table(pyarrow.table(columns), sink)
    return sink.getvalue()


def order_fixture(**overrides) -> Fixture:
    base = {
        "files": {
            "order.csv": ORDER_CSV,
            "order.parquet": parquet_bytes({"id": ["o1", "o2"], "amount": [40, 150]}),
        },
        "schemas": {"order": ("id", "amount")},
        "sources": {"order": ("order.csv", "order.parquet")},
    }
    return Fixture(**{**base, **overrides})


def test_a_relation_may_ship_in_two_formats():
    verify(order_fixture())


def test_a_parquet_copy_with_a_renamed_column_is_caught():
    fixture = order_fixture(
        files={
            "order.csv": ORDER_CSV,
            "order.parquet": parquet_bytes({"id": ["o1", "o2"], "total": [40, 150]}),
        }
    )
    with pytest.raises(SchemaDrift):
        verify(fixture)


def test_copies_that_disagree_on_row_count_are_caught():
    # The failure a redundant copy invites: the text file is regenerated, the
    # binary one is not, and both still match the schema.
    fixture = order_fixture(
        files={
            "order.csv": ORDER_CSV,
            "order.parquet": parquet_bytes({"id": ["o1"], "amount": [40]}),
        }
    )
    with pytest.raises(SchemaDrift):
        verify(fixture)


def test_a_jsonl_relation_is_checked_against_its_first_object():
    good = Fixture(
        files={"shipment.jsonl": '{"id": "s1", "order": "o1"}\n{"id": "s2", "order": "o2"}\n'},
        schemas={"shipment": ("id", "order")},
        sources={"shipment": ("shipment.jsonl",)},
    )
    verify(good)

    drifted = Fixture(
        files={"shipment.jsonl": '{"id": "s1", "order_id": "o1"}\n'},
        schemas={"shipment": ("id", "order")},
        sources={"shipment": ("shipment.jsonl",)},
    )
    with pytest.raises(SchemaDrift):
        verify(drifted)


def test_a_data_file_with_no_schema_is_still_caught_in_any_format():
    fixture = order_fixture(
        files={
            "order.csv": ORDER_CSV,
            "order.parquet": parquet_bytes({"id": ["o1", "o2"], "amount": [40, 150]}),
            "shipment.jsonl": '{"id": "s1"}\n',
        }
    )
    with pytest.raises(SchemaDrift):
        verify(fixture)


def test_a_format_the_engine_cannot_import_is_rejected():
    fixture = Fixture(
        files={"order.tsv": "id\tamount\n"},
        schemas={"order": ("id", "amount")},
        sources={"order": ("order.tsv",)},
    )
    with pytest.raises(SchemaDrift):
        verify(fixture)


def test_a_source_tree_needs_no_schema_and_is_summarized_not_listed():
    fixture = Fixture(
        files={
            "order.csv": ORDER_CSV,
            "pkg/__init__.py": "from pkg import core\n",
            "pkg/core.py": "def run():\n    return 1\n",
        },
        schemas={"order": ("id", "amount")},
    )
    verify(fixture)  # the tree is an asset, not an undeclared relation

    catalogue = relation_catalogue(fixture)
    assert "`order.csv` (2 rows)" in catalogue
    assert "`pkg`" in catalogue
    assert "2 files" in catalogue
    # The subject walks the tree; handing it an index would be answering part of
    # the extraction for it.
    assert "core.py" not in catalogue
