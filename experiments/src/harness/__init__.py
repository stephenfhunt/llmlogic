"""The S1 measuring harness.

Runs each task twice — once by an agent that has the ``datalog`` engine, once by
the same agent without it — and grades both against ground truth computed
independently of the engine. See ``../README.md`` for what a cell is and
``../AGENTS.md`` for the controls that make the measurement valid.
"""

__version__ = "0.1.0"
