//! Abstract syntax tree for Datalog programs — the *surface* layer.
//!
//! This tree mirrors the grammar (`spec.md` §5) and lexical structure (§3)
//! exactly: it carries source spans on every node, keeps named arguments and
//! wildcards, and represents programs as the user wrote them. It is the output
//! of the parser and the input to front-end lowering ([`crate::lower`]).
//!
//! The evaluator never sees this tree — it consumes the positional, resolved
//! core IR ([`crate::ir`]) instead (spec §17, 2026-07-19: surface AST / core IR
//! split). Consequently these types make surface-only distinctions first-class:
//! [`Args`] is an enum so a literal is all-positional or all-named by
//! construction (never mixed, §4), and [`TermKind::Wildcard`] survives until
//! lowering eliminates it.
//!
//! Provisional grammar is *not* represented: aggregate expressions (§9) will
//! land as a new [`LiteralKind`] variant, and provenance queries (`?why`, §11)
//! as a new [`StatementKind`] variant, once their syntax is ratified. Operator
//! precedence (§8) is a parser concern; [`Expr`] represents any parse.

/// A half-open byte range `[start, end)` into the program source.
///
/// Spans power the structured error model (§12): every node carries one so
/// semantic errors reported long after parsing can still point at source text.
/// Hand-constructed trees (engine and lowering tests predate the parser) use
/// [`Span::DUMMY`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// The placeholder span used by hand-constructed fixtures.
    pub const DUMMY: Span = Span { start: 0, end: 0 };
}

/// An identifier (relation name, field name, or symbol spelling) with its span.
#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A whole program: a sequence of `.`-terminated statements (§5).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub statements: Vec<Statement>,
}

/// A single statement with its span.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

/// The four statement forms of §5.
///
/// A future provenance query statement (`?why …`, §11/§16.6, provisional) will
/// become a new variant here once its syntax is ratified.
#[derive(Debug, Clone, PartialEq)]
pub enum StatementKind {
    /// `import "<path>" as rel [ (field [: type], …) ].` (§13)
    Import(Import),
    /// `declare rel(field [: type], …).` (§4)
    Declare(Declaration),
    /// A fact (empty body) or rule.
    Clause(Clause),
    /// `?- body.`
    Query(Query),
}

/// An `import` statement binding an external source to a relation (§13).
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    /// The source path as written (contents of the string literal).
    pub path: String,
    /// Span of the path string literal, for path-related errors.
    pub path_span: Span,
    pub relation: Ident,
    /// Explicit schema override; `None` means infer from the source (§13).
    pub schema: Option<Vec<FieldDecl>>,
}

/// A `declare` statement naming fields and optionally asserting types (§4).
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub relation: Ident,
    pub fields: Vec<FieldDecl>,
}

/// One field in a `declare` or import schema: a name and an optional type.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: Ident,
    pub ty: Option<TypeName>,
    pub span: Span,
}

/// The five primitive type names (§4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeName {
    Symbol,
    String,
    Int,
    Float,
    Bool,
}

/// A clause: a fact when `body` is empty, otherwise a rule (§5).
#[derive(Debug, Clone, PartialEq)]
pub struct Clause {
    pub head: Atom,
    pub body: Vec<Literal>,
    pub span: Span,
}

/// A query statement: `?- body.`
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub body: Vec<Literal>,
    pub span: Span,
}

/// A predicate applied to arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct Atom {
    pub predicate: Ident,
    pub args: Args,
    pub span: Span,
}

/// Atom arguments: all-positional or all-named, never mixed (§4).
///
/// The "never mixed" rule is a type invariant here, not a check — the parser
/// reports mixing as a targeted syntax error (§5) and this enum cannot
/// represent it.
#[derive(Debug, Clone, PartialEq)]
pub enum Args {
    Positional(Vec<Term>),
    /// Named arguments; partial selection is allowed in bodies (§4). Lowering
    /// resolves these to positional form against the predicate schema.
    Named(Vec<NamedArg>),
}

/// One `field: value` pair in a named-argument literal.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedArg {
    pub field: Ident,
    pub value: Term,
    pub span: Span,
}

/// A body literal with its span.
#[derive(Debug, Clone, PartialEq)]
pub struct Literal {
    pub kind: LiteralKind,
    pub span: Span,
}

/// The body literal forms of §5.
///
/// A future aggregate expression (§9) will become a new variant here once its
/// syntax is ratified (the provisional `count { V : Goal }` form collides with
/// the named-argument `:` and is being revisited).
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralKind {
    /// A possibly negated atom; `not` applies to atoms only (§5).
    Atom { negated: bool, atom: Atom },
    /// A comparison between two arithmetic expressions.
    Comparison(Comparison),
}

/// A comparison literal: `expr cmp expr`.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub op: CmpOp,
    pub lhs: Expr,
    pub rhs: Expr,
}

/// Comparison operators: `=` `!=` `<` `<=` `>` `>=` (§3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Arithmetic operators: `+` `-` `*` `/` (§3; semantics §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// An arithmetic expression with its span.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// Expression forms. Operator precedence (§8, TBD) is resolved by the parser;
/// this tree represents whatever grouping it chose.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Term(Term),
    Binary {
        op: ArithOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

/// A term with its span. Terms are flat (§4): constant or variable, no
/// compound/function terms in v1.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    pub kind: TermKind,
    pub span: Span,
}

/// Term forms (§3).
#[derive(Debug, Clone, PartialEq)]
pub enum TermKind {
    Constant(Constant),
    /// A named variable (uppercase- or `_`-initial, but not a lone `_`).
    Variable(String),
    /// A lone `_`: each occurrence is a fresh anonymous variable, eliminated
    /// during lowering.
    Wildcard,
}

/// A constant of one of the five primitive types (§4).
///
/// `Float` holds a raw `f64` — surface nodes are never set members, so total
/// equality/hashing is not needed here (the core IR's `Value` is where float
/// totality is enforced).
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    /// A bare identifier in term position (§3); distinct from `String`.
    Symbol(String),
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Hand-constructed AST fixtures from the spec §16 corpus, shared with the
    //! lowering tests. Plain verbatim construction — no builders, per
    //! AGENTS.md testing conventions.

    use super::*;

    pub(crate) fn ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn string_term(s: &str) -> Term {
        Term {
            kind: TermKind::Constant(Constant::String(s.to_string())),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn var_term(name: &str) -> Term {
        Term {
            kind: TermKind::Variable(name.to_string()),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn positional_atom(predicate: &str, args: Vec<Term>) -> Atom {
        Atom {
            predicate: ident(predicate),
            args: Args::Positional(args),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn positive_literal(atom: Atom) -> Literal {
        Literal {
            kind: LiteralKind::Atom {
                negated: false,
                atom,
            },
            span: Span::DUMMY,
        }
    }

    pub(crate) fn fact(predicate: &str, args: Vec<Term>) -> Statement {
        Statement {
            kind: StatementKind::Clause(Clause {
                head: positional_atom(predicate, args),
                body: Vec::new(),
                span: Span::DUMMY,
            }),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn rule(head: Atom, body: Vec<Literal>) -> Statement {
        Statement {
            kind: StatementKind::Clause(Clause {
                head,
                body,
                span: Span::DUMMY,
            }),
            span: Span::DUMMY,
        }
    }

    /// Spec §16.1 — ancestry: three facts, a non-recursive rule, a recursive
    /// rule, and a query, exactly as the parser will produce them.
    pub(crate) fn example_16_1() -> Program {
        Program {
            statements: vec![
                fact("parent", vec![string_term("alice"), string_term("bob")]),
                fact("parent", vec![string_term("bob"), string_term("carol")]),
                fact("parent", vec![string_term("carol"), string_term("dave")]),
                rule(
                    positional_atom("ancestor", vec![var_term("X"), var_term("Y")]),
                    vec![positive_literal(positional_atom(
                        "parent",
                        vec![var_term("X"), var_term("Y")],
                    ))],
                ),
                rule(
                    positional_atom("ancestor", vec![var_term("X"), var_term("Y")]),
                    vec![
                        positive_literal(positional_atom(
                            "parent",
                            vec![var_term("X"), var_term("Z")],
                        )),
                        positive_literal(positional_atom(
                            "ancestor",
                            vec![var_term("Z"), var_term("Y")],
                        )),
                    ],
                ),
                Statement {
                    kind: StatementKind::Query(Query {
                        body: vec![positive_literal(positional_atom(
                            "ancestor",
                            vec![string_term("alice"), var_term("Who")],
                        ))],
                        span: Span::DUMMY,
                    }),
                    span: Span::DUMMY,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn example_16_1_has_expected_shape() {
        let program = example_16_1();
        assert_eq!(program.statements.len(), 6);

        let clauses: Vec<&Clause> = program
            .statements
            .iter()
            .filter_map(|s| match &s.kind {
                StatementKind::Clause(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(clauses.len(), 5);
        let (facts, rules): (Vec<&Clause>, Vec<&Clause>) =
            clauses.into_iter().partition(|c| c.body.is_empty());
        assert_eq!(facts.len(), 3);
        assert_eq!(rules.len(), 2);

        // The recursive rule's second body literal references the head predicate.
        let recursive = rules[1];
        assert_eq!(recursive.head.predicate.name, "ancestor");
        match &recursive.body[1].kind {
            LiteralKind::Atom { negated, atom } => {
                assert!(!negated);
                assert_eq!(atom.predicate.name, "ancestor");
            }
            other => panic!("expected an atom literal, got {other:?}"),
        }

        let queries = program
            .statements
            .iter()
            .filter(|s| matches!(s.kind, StatementKind::Query(_)))
            .count();
        assert_eq!(queries, 1);
    }

    /// Spec §16.7 fragment — `declare person(name: string, age: int).` plus
    /// `adult(N) :- person(name: N, age: A), A >= 18.` exercising `declare`,
    /// named arguments, and a comparison literal. Mixing positional and named
    /// arguments in one atom is unrepresentable: `Args` forces the choice.
    #[test]
    fn example_16_7_named_arguments_shape() {
        let declare = Statement {
            kind: StatementKind::Declare(Declaration {
                relation: ident("person"),
                fields: vec![
                    FieldDecl {
                        name: ident("name"),
                        ty: Some(TypeName::String),
                        span: Span::DUMMY,
                    },
                    FieldDecl {
                        name: ident("age"),
                        ty: Some(TypeName::Int),
                        span: Span::DUMMY,
                    },
                ],
            }),
            span: Span::DUMMY,
        };
        let adult_rule = rule(
            positional_atom("adult", vec![var_term("N")]),
            vec![
                Literal {
                    kind: LiteralKind::Atom {
                        negated: false,
                        atom: Atom {
                            predicate: ident("person"),
                            args: Args::Named(vec![
                                NamedArg {
                                    field: ident("name"),
                                    value: var_term("N"),
                                    span: Span::DUMMY,
                                },
                                NamedArg {
                                    field: ident("age"),
                                    value: var_term("A"),
                                    span: Span::DUMMY,
                                },
                            ]),
                            span: Span::DUMMY,
                        },
                    },
                    span: Span::DUMMY,
                },
                Literal {
                    kind: LiteralKind::Comparison(Comparison {
                        op: CmpOp::Ge,
                        lhs: Expr {
                            kind: ExprKind::Term(var_term("A")),
                            span: Span::DUMMY,
                        },
                        rhs: Expr {
                            kind: ExprKind::Term(Term {
                                kind: TermKind::Constant(Constant::Int(18)),
                                span: Span::DUMMY,
                            }),
                            span: Span::DUMMY,
                        },
                    }),
                    span: Span::DUMMY,
                },
            ],
        );
        let program = Program {
            statements: vec![declare, adult_rule],
        };

        match &program.statements[0].kind {
            StatementKind::Declare(d) => {
                assert_eq!(d.relation.name, "person");
                assert_eq!(d.fields.len(), 2);
                assert_eq!(d.fields[0].ty, Some(TypeName::String));
            }
            other => panic!("expected a declaration, got {other:?}"),
        }
        match &program.statements[1].kind {
            StatementKind::Clause(c) => match &c.body[0].kind {
                LiteralKind::Atom { atom, .. } => match &atom.args {
                    Args::Named(named) => {
                        assert_eq!(named.len(), 2);
                        assert_eq!(named[0].field.name, "name");
                    }
                    Args::Positional(_) => panic!("expected named arguments"),
                },
                other => panic!("expected an atom literal, got {other:?}"),
            },
            other => panic!("expected a clause, got {other:?}"),
        }
    }
}
