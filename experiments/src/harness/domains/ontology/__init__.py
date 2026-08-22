"""A class hierarchy, its instances, and what they inherit.

Subsumption is the other half of transitive closure. `access_control` follows a
chain of *grants*; this follows a chain of *kinds*, and adds the two things that
make an ontology hard to reason about in prose:

- **multiple inheritance**, so "the ancestors of X" is a set rather than a path,
  and a class reached by two routes is easy to count twice and easy to miss;
- **overriding**, where the value that applies is the one declared on the most
  specific ancestor — a rule a prose reading satisfies by taking the first
  declaration it happens to see.

The disjointness question is a consistency check over the same closure: an
instance that lands in two classes declared disjoint is a data error, and nothing
in the fact base announces it.
"""
