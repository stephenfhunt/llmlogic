"""Tables that arrive from outside, with dates, gaps and money in them.

The other packs hand the subject a graph. This one hands it the shape most real
fact bases have — a few tables joined by keys, dates that have to be subtracted,
amounts that are sometimes simply missing — and asks questions whose answers are
aggregates rather than paths.

Three things make it its own domain rather than a variation:

- **Dates are values, not strings.** "More than five days later" is arithmetic on
  a type, and a run that treats the column as text gets a plausible answer.
- **A missing amount is not a zero.** Some rows have no amount at all, and every
  question that sums says what to do with them. An arm that reads a blank cell as
  `0` answers a different question and nothing in the output says so.
- **The tables ship in more than one format.** `order` is present as CSV *and* as
  Parquet, the same rows in each; `shipment` is JSONL. The Parquet copy is
  redundant on purpose — a table only one arm can open would decide cells on file
  format rather than on reasoning (`decisions.md`, 2026-08-22).
"""
