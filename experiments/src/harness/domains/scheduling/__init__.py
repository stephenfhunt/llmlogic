"""A roster, and the four things anyone actually asks about one.

Constraint checking is what a Datalog engine offers a scheduler: there is no
`constraint` keyword because a query already is one, and a roster is where that
matters commercially. The questions here are the ones a shift manager asks —
who is double-booked, which shifts nobody can cover, which shifts have only one
possible person, and who is being turned round too fast.

Two properties make it a different domain from the other packs rather than a
retheme:

- **Intervals overlap; keys do not.** Every question compares two rows of the
  same relation on a pair of times, and the comparison is the whole difficulty.
- **The interesting answers are about absence.** An unstaffable shift is defined
  by nobody satisfying two conditions at once, which is a negation over a join
  and the classic place a hand-check reports "looks fine".

Deliberately *not* a logic-grid puzzle. The skill ships one as an example
(`datalog/skill/examples/houses_puzzle.dl`), and a question the guide already
contains is one the engine arm can pattern-match instead of solve.
"""
