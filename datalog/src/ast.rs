//! Abstract syntax tree for Datalog programs.
//!
//! Placeholder. The concrete term/atom/rule/program types are defined by the data
//! model (`spec.md` §4) and syntax (`spec.md` §5). Left empty until those sections
//! stabilize so the AST reflects the finalized design rather than guesses.
//!
//! Anticipated types (TODO):
//! - `Term`      — constant (symbol / string / integer / float / bool) or variable
//! - `Atom`      — a predicate applied to terms
//! - `Literal`   — a (possibly negated) atom or builtin comparison
//! - `Rule`      — head atom `:-` body of literals
//! - `Program`   — a set of rules, facts, and declarations
