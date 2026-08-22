"""Who can do what, through groups and a role hierarchy.

The canonical industrial Datalog domain, and in the slate for four reasons: the
question is genuinely multi-hop (user → group → role → *transitively* included
roles → permission), it needs negation over a closed set, its ground truth is a
twenty-line breadth-first search, and the skill's own documentation never uses it
— so a subject cannot pattern-match a program out of the guide.
"""
