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
    /// Positional arguments. Each is a full [`Expr`], not just a [`Term`], so
    /// inline arithmetic parses (`succ(N, N+1)`, spec §17 Phase D). A bare term
    /// is the common case, represented as [`ExprKind::Term`]; lowering hoists any
    /// non-term argument to a fresh `=`-assignment body literal, so the IR and
    /// engine still see only positional terms.
    Positional(Vec<Expr>),
    /// Named arguments; partial selection is allowed in bodies (§4). Lowering
    /// resolves these to positional form against the predicate schema.
    Named(Vec<NamedArg>),
}

/// One `field: value` pair in a named-argument literal. The value is a full
/// [`Expr`] for the same reason positional arguments are (inline arithmetic).
#[derive(Debug, Clone, PartialEq)]
pub struct NamedArg {
    pub field: Ident,
    pub value: Expr,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    /// The canonical source spelling of this operator.
    pub fn symbol(self) -> &'static str {
        match self {
            CmpOp::Eq => "=",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
    }
}

/// The canonical source spelling of a comparison operator.
pub fn cmp_symbol(op: CmpOp) -> &'static str {
    op.symbol()
}

/// Arithmetic operators: `+` `-` `*` `/` (§3; semantics §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl ArithOp {
    /// The canonical source spelling of this operator.
    pub fn symbol(self) -> &'static str {
        match self {
            ArithOp::Add => "+",
            ArithOp::Sub => "-",
            ArithOp::Mul => "*",
            ArithOp::Div => "/",
        }
    }
}

/// The canonical source spelling of an arithmetic operator.
pub fn arith_symbol(op: ArithOp) -> &'static str {
    op.symbol()
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

    /// Wraps a leaf term as an expression — atom arguments are [`Expr`] since
    /// the inline-arithmetic widening (spec §17, Phase D). Fixtures keep passing
    /// bare terms; this is where they become `Expr::Term`.
    pub(crate) fn expr_of(term: Term) -> Expr {
        Expr {
            span: term.span,
            kind: ExprKind::Term(term),
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

    pub(crate) fn int_term(value: i64) -> Term {
        Term {
            kind: TermKind::Constant(Constant::Int(value)),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn named_arg(field: &str, value: Term) -> NamedArg {
        NamedArg {
            field: ident(field),
            value: expr_of(value),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn named_atom(predicate: &str, args: Vec<NamedArg>) -> Atom {
        Atom {
            predicate: ident(predicate),
            args: Args::Named(args),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn field_decl(name: &str, ty: Option<TypeName>) -> FieldDecl {
        FieldDecl {
            name: ident(name),
            ty,
            span: Span::DUMMY,
        }
    }

    pub(crate) fn declare(relation: &str, fields: Vec<FieldDecl>) -> Statement {
        Statement {
            kind: StatementKind::Declare(Declaration {
                relation: ident(relation),
                fields,
            }),
            span: Span::DUMMY,
        }
    }

    pub(crate) fn positional_atom(predicate: &str, args: Vec<Term>) -> Atom {
        Atom {
            predicate: ident(predicate),
            args: Args::Positional(args.into_iter().map(expr_of).collect()),
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

    pub(crate) fn negated_literal(atom: Atom) -> Literal {
        Literal {
            kind: LiteralKind::Atom {
                negated: true,
                atom,
            },
            span: Span::DUMMY,
        }
    }

    pub(crate) fn wildcard_term() -> Term {
        Term {
            kind: TermKind::Wildcard,
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

    pub(crate) fn query(body: Vec<Literal>) -> Statement {
        Statement {
            kind: StatementKind::Query(Query {
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

    /// Spec §16.2 — stratified negation-as-failure: `root` is everyone with no
    /// recorded parent, via a negated atom with a wildcard existential under
    /// the negation (§7).
    pub(crate) fn example_16_2() -> Program {
        Program {
            statements: vec![
                fact("person", vec![string_term("alice")]),
                fact("person", vec![string_term("bob")]),
                fact("person", vec![string_term("carol")]),
                fact("parent", vec![string_term("alice"), string_term("bob")]),
                fact("parent", vec![string_term("bob"), string_term("carol")]),
                // root(X) :- person(X), not parent(_, X).
                rule(
                    positional_atom("root", vec![var_term("X")]),
                    vec![
                        positive_literal(positional_atom("person", vec![var_term("X")])),
                        negated_literal(positional_atom(
                            "parent",
                            vec![wildcard_term(), var_term("X")],
                        )),
                    ],
                ),
            ],
        }
    }

    /// Spec §16.7 — named arguments and partial selection: a wide imported
    /// table selected by field name, and a `declare`d in-program predicate used
    /// the same way.
    ///
    /// One adaptation from the spec text: §16.7 writes the import without a
    /// schema and takes field names from the CSV header. Header inference needs
    /// fact sources (§13, not yet implemented), so this fixture uses the
    /// explicit-schema import form — which also exercises the `Import` schema
    /// origin alongside `Declare`.
    pub(crate) fn example_16_7() -> Program {
        Program {
            statements: vec![
                Statement {
                    kind: StatementKind::Import(Import {
                        path: "data/employees.csv".to_string(),
                        path_span: Span::DUMMY,
                        relation: ident("employee"),
                        schema: Some(vec![
                            field_decl("id", Some(TypeName::Int)),
                            field_decl("name", Some(TypeName::String)),
                            field_decl("age", Some(TypeName::Int)),
                            field_decl("dept", Some(TypeName::String)),
                            field_decl("title", Some(TypeName::String)),
                            field_decl("salary", Some(TypeName::Int)),
                            field_decl("city", Some(TypeName::String)),
                            field_decl("start_date", Some(TypeName::String)),
                        ]),
                    }),
                    span: Span::DUMMY,
                },
                // manager_name(N) :- employee(name: N, title: "manager").
                // Two fields selected out of eight — no wildcard run.
                rule(
                    positional_atom("manager_name", vec![var_term("N")]),
                    vec![positive_literal(named_atom(
                        "employee",
                        vec![
                            named_arg("name", var_term("N")),
                            named_arg("title", string_term("manager")),
                        ],
                    ))],
                ),
                declare(
                    "person",
                    vec![
                        field_decl("name", Some(TypeName::String)),
                        field_decl("age", Some(TypeName::Int)),
                    ],
                ),
                fact("person", vec![string_term("alice"), int_term(30)]),
                // adult(N) :- person(name: N, age: A), A >= 18.
                rule(
                    positional_atom("adult", vec![var_term("N")]),
                    vec![
                        positive_literal(named_atom(
                            "person",
                            vec![
                                named_arg("name", var_term("N")),
                                named_arg("age", var_term("A")),
                            ],
                        )),
                        Literal {
                            kind: LiteralKind::Comparison(Comparison {
                                op: CmpOp::Ge,
                                lhs: Expr {
                                    kind: ExprKind::Term(var_term("A")),
                                    span: Span::DUMMY,
                                },
                                rhs: Expr {
                                    kind: ExprKind::Term(int_term(18)),
                                    span: Span::DUMMY,
                                },
                            }),
                            span: Span::DUMMY,
                        },
                    ],
                ),
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
    /// Spec §16.7 — `declare`, named arguments, and a comparison literal.
    /// Mixing positional and named arguments in one atom is unrepresentable:
    /// `Args` forces the choice.
    #[test]
    fn example_16_7_named_arguments_shape() {
        let program = example_16_7();

        // The wide import carries an explicit eight-field schema.
        match &program.statements[0].kind {
            StatementKind::Import(import) => {
                let schema = import.schema.as_ref().expect("explicit schema");
                assert_eq!(schema.len(), 8);
                assert_eq!(schema[4].name.name, "title");
            }
            other => panic!("expected an import, got {other:?}"),
        }

        // Partial selection: two of eight fields named in the body literal.
        match &program.statements[1].kind {
            StatementKind::Clause(c) => match &c.body[0].kind {
                LiteralKind::Atom { atom, .. } => match &atom.args {
                    Args::Named(named) => {
                        assert_eq!(named.len(), 2);
                        assert_eq!(named[0].field.name, "name");
                        assert_eq!(named[1].field.name, "title");
                    }
                    Args::Positional(_) => panic!("expected named arguments"),
                },
                other => panic!("expected an atom literal, got {other:?}"),
            },
            other => panic!("expected a clause, got {other:?}"),
        }

        match &program.statements[2].kind {
            StatementKind::Declare(d) => {
                assert_eq!(d.relation.name, "person");
                assert_eq!(d.fields.len(), 2);
                assert_eq!(d.fields[0].ty, Some(TypeName::String));
            }
            other => panic!("expected a declaration, got {other:?}"),
        }

        // The `adult` rule pairs a named literal with a comparison.
        match &program.statements[4].kind {
            StatementKind::Clause(c) => {
                assert_eq!(c.body.len(), 2);
                assert!(matches!(c.body[1].kind, LiteralKind::Comparison(_)));
            }
            other => panic!("expected a clause, got {other:?}"),
        }
    }
}
