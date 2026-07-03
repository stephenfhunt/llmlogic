//! Lexer: source text → token stream.
//!
//! Placeholder pending the lexical structure (`spec.md` §3). Will emit tokens with
//! source spans so parser and engine errors can point precisely at offending text
//! (a requirement of the structured-error pillar). The lexer approach (hand-rolled
//! vs a crate such as `logos`) is an open decision — see `spec.md` §17.
