"""A dozen rows in two relations. Small on purpose.

Two entry points, answering different questions.

``build()`` returns the **pinned** books — six employees, six orders, and the
four questions written against them by hand.

``generate(seed, difficulty, track)`` returns a fresh set. It is the odd
generator in the slate, and deliberately so: **a control that gets hard stops
being a control.** These are questions a model answers in its head, so
difficulty varies the names and the values and grows the table by a few rows,
and never the number of hops. There is no `at-scale` track at all — a
single-hop lookup in a 100,000-row table would be a question about retrieval,
and a positive delta there would have an innocent explanation the null is
supposed to rule out.
"""

from __future__ import annotations

import random
from dataclasses import dataclass

from harness.task import Fixture

EMPLOYEES: tuple[tuple[str, str, int], ...] = (
    ("alice", "engineering", 120000),
    ("bob", "sales", 90000),
    ("carol", "engineering", 135000),
    ("dave", "support", 70000),
    ("erin", "engineering", 110000),
    ("frank", "sales", 95000),
)

ORDERS: tuple[tuple[str, str, int], ...] = (
    ("o1", "acme", 40),
    ("o2", "borogove", 150),
    ("o3", "acme", 99),
    ("o4", "cyrus", 220),
    ("o5", "borogove", 101),
    ("o6", "cyrus", 15),
)

#: ``(employees, orders, departments, customers)``. The rows grow a little so a
#: calibrated slate has some range to select over; nothing else moves. Twenty
#: rows is still a table a model reads in one look, which is the property that
#: makes these controls rather than questions.
DIFFICULTY: dict[int, tuple[int, int, int, int]] = {
    1: (6, 6, 3, 3),  # the pinned shape
    2: (9, 9, 3, 3),
    3: (12, 12, 4, 4),
    4: (16, 16, 4, 4),
    5: (20, 20, 5, 5),
}


@dataclass(frozen=True)
class Books:
    """Two small tables, and the threshold the arithmetic question uses."""

    employees: tuple[tuple[str, str, int], ...]
    orders: tuple[tuple[str, str, int], ...]
    departments: tuple[str, ...]
    high_order_threshold: int = 100
    #: The employee, order and department the four questions are asked about,
    #: chosen so each has a non-empty answer. Empty on the pinned books, whose
    #: questions name their subjects by hand.
    subject_employee: str = ""
    subject_order: str = ""
    subject_department: str = ""


PINNED = Books(
    employees=EMPLOYEES,
    orders=ORDERS,
    departments=tuple(sorted({department for _, department, _ in EMPLOYEES})),
)


def generate(seed: int, difficulty: int = 1, track: str = "in-context") -> Books:
    """A fresh set of books.

    Identifiers are regenerated per seed, so nothing here can be answered from
    having seen it before — which matters as much for a control as for a
    measured item, because a memorised control calibrates nothing.
    """
    if difficulty not in DIFFICULTY:
        raise ValueError(f"difficulty must be one of {sorted(DIFFICULTY)}")
    if track != "in-context":
        raise ValueError(
            "controls have no at-scale track: a single-hop lookup in a fact base "
            "nobody can hold is a question about retrieval, and a control that "
            "gets hard stops being a control"
        )
    n_employees, n_orders, n_departments, n_customers = DIFFICULTY[difficulty]

    rng = random.Random(seed)
    tag = f"{seed:x}"[-4:]
    departments = tuple(f"d{tag}{index}" for index in range(n_departments))
    customers = tuple(f"k{tag}{index}" for index in range(n_customers))

    # Round-robin rather than drawn, so every department has at least one member
    # and *how many are in this department?* never answers zero.
    employees = tuple(
        (f"e{tag}{index:02d}", departments[index % n_departments], rng.randrange(60, 160) * 1000)
        for index in range(n_employees)
    )
    orders = tuple(
        (f"o{tag}{index:02d}", customers[index % n_customers], rng.randint(10, 300))
        for index in range(n_orders)
    )

    # A threshold with roughly half the orders above it: fixed at 100 against a
    # draw that happens to sit above it, *which orders exceed this?* answers with
    # every row and copying a column would score correct.
    amounts = sorted(amount for _, _, amount in orders)
    threshold = amounts[len(amounts) // 2]
    return Books(
        employees=employees,
        orders=orders,
        departments=departments,
        high_order_threshold=threshold,
        subject_employee=employees[len(employees) // 2][0],
        subject_order=orders[len(orders) // 2][0],
        subject_department=departments[0],
    )


def to_fixture(books: Books) -> Fixture:
    """The two CSVs a workspace gets. One place, so a generated set and the
    pinned one are laid out identically."""
    return Fixture(
        files={
            "employee.csv": "name,department,salary\n"
            + "".join(
                f"{name},{department},{salary}\n" for name, department, salary in books.employees
            ),
            "order.csv": "id,customer,amount\n"
            + "".join(
                f"{order_id},{customer},{amount}\n" for order_id, customer, amount in books.orders
            ),
        },
        schemas={
            "employee": ("name", "department", "salary"),
            "order": ("id", "customer", "amount"),
        },
    )


def build() -> Fixture:
    return to_fixture(PINNED)
