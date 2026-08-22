"""Ground truth, in plain Python. Never the datalog engine (control 1)."""

from __future__ import annotations

from harness.domains.controls.fixture import EMPLOYEES, ORDERS
from harness.task import Answer

HIGH_ORDER_THRESHOLD = 100


def department_of(person: str) -> Answer:
    return Answer.of(*[department for name, department, _ in EMPLOYEES if name == person])


def orders_above(threshold: int = HIGH_ORDER_THRESHOLD) -> Answer:
    return Answer.of(*[order_id for order_id, _, amount in ORDERS if amount > threshold])


def headcount(department: str) -> Answer:
    count = sum(1 for _, actual, _ in EMPLOYEES if actual == department)
    return Answer.of(str(count))


def customer_of(order: str) -> Answer:
    return Answer.of(*[customer for order_id, customer, _ in ORDERS if order_id == order])
