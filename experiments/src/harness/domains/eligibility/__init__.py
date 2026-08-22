"""Who qualifies, who does not, and who cannot be told either way.

Rules-over-records is the shape most business logic actually has, and it is
where two failure modes live that a graph domain cannot reach:

- **A missing value is not a small value.** Some applicants have no income
  recorded. Every comparison against that is false, in both directions — so they
  are neither eligible nor ineligible, and an arm that reads a blank cell as
  ``0`` finds them eligible with total confidence.
- **"Why not" is a different question from "not".** One task asks which single
  criterion each near-miss fails, which is the eligibility question people
  actually ask and the one that needs the failures counted rather than found.

The criteria are stated in the question rather than carried in a table. Rules as
data would make both arms write an interpreter, and measure that instead.
"""
