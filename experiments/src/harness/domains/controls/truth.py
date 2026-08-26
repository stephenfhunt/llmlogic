"""Ground truth, in plain Python. Never the datalog engine (control 1)."""

from __future__ import annotations

from harness.domains.controls.fixture import PINNED, Books
from harness.task import Answer

HIGH_ORDER_THRESHOLD = PINNED.high_order_threshold

#: The pinned tables, under the names the four pinned questions are written
#: against.
EMPLOYEES = PINNED.employees
ORDERS = PINNED.orders


def department_of(person: str, books: Books = PINNED) -> Answer:
    return Answer.of(*[department for name, department, _ in books.employees if name == person])


def orders_above(threshold: int | None = None, books: Books = PINNED) -> Answer:
    limit = books.high_order_threshold if threshold is None else threshold
    return Answer.of(*[order_id for order_id, _, amount in books.orders if amount > limit])


def headcount(department: str, books: Books = PINNED) -> Answer:
    count = sum(1 for _, actual, _ in books.employees if actual == department)
    return Answer.of(str(count))


def customer_of(order: str, books: Books = PINNED) -> Answer:
    return Answer.of(*[customer for order_id, customer, _ in books.orders if order_id == order])
