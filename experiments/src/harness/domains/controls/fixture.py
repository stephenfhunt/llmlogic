"""A dozen rows in two relations. Small on purpose."""

from __future__ import annotations

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


def build() -> Fixture:
    employee_csv = "name,department,salary\n" + "".join(
        f"{name},{department},{salary}\n" for name, department, salary in EMPLOYEES
    )
    order_csv = "id,customer,amount\n" + "".join(
        f"{order_id},{customer},{amount}\n" for order_id, customer, amount in ORDERS
    )
    return Fixture(
        files={"employee.csv": employee_csv, "order.csv": order_csv},
        schemas={
            "employee": ("name", "department", "salary"),
            "order": ("id", "customer", "amount"),
        },
    )
