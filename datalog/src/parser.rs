//! Hand-rolled recursive-descent parser: tokens → surface [`ast::Program`]
//! (`spec.md` §5).
//!
//! The parser targets the **existing** surface AST — a fixed contract the
//! engine already consumes through lowering — never a redesign. Zero
//! dependencies (spec §17, Phase D).
//!
//! Design points:
//! - **Statement-level recovery.** A malformed statement is reported and the
//!   parser skips to the next `.`, so one run surfaces *every* error (§12
//!   pillar), not just the first. [`parse`] returns `Err(Vec<Error>)` carrying
//!   both lexical and syntactic errors.
//! - **Precedence climbing** for arithmetic: `*` `/` bind tighter than `+` `-`,
//!   both left-associative; comparisons are non-associative and do not chain
//!   (spec §17, Phase D, decision 2).
//! - **Signed literals fold in the parser** (decision 3): a prefix `-` on a
//!   numeric literal becomes a negative [`ast::Constant`]; there is no
//!   unary-minus AST node. A prefix `-` on anything else is a structured error.
//! - **Disjunction `;` in rule bodies** expands here: a body of *k* disjuncts
//!   becomes *k* [`ast::Clause`]s sharing the head, so the AST stays
//!   conjunction-only (decision 7). Queries stay conjunctive.
//! - **Inline arithmetic** parses because atom arguments are full
//!   [`ast::Expr`]s (the widening); lowering hoists the compound ones.
//! - **Strict grammar with did-you-mean errors** for the Prolog-prior
//!   near-misses (uppercase relation, chained comparison, `not` before a
//!   comparison, zero-arity atom, mixed argument styles, trailing comma); the
//!   operator near-misses (`=<`, `\=`, `\+`, `!`) are already handled in the
//!   lexer.

use crate::ast::{
    AggOp, Aggregate, Args, Atom, Clause, Comparison, Constant, Declaration, Expr, ExprKind,
    FieldDecl, Ident, Import, ImportKind, Literal, LiteralKind, NamedArg, Program, Query, Span,
    Statement, StatementKind, Term, TermKind, TypeName,
};
use crate::error::Error;
use crate::lexer::{Token, TokenKind, lex};

/// Parses source text to a surface program, or reports every lexical and
/// syntactic error found (spec §12 pillar; never panics, `testing.md` D4).
pub fn parse(src: &str) -> Result<Program, Vec<Error>> {
    let lexed = lex(src);
    let mut parser = Parser {
        tokens: lexed.tokens,
        pos: 0,
        errors: Vec::new(),
    };
    let program = parser.parse_program();
    let mut errors = lexed.errors;
    errors.append(&mut parser.errors);
    if errors.is_empty() {
        Ok(program)
    } else {
        Err(errors)
    }
}

/// A parse step that failed and has already recorded its error. The unit
/// payload keeps error *reporting* at the failure site and *recovery* at the
/// statement loop.
type PResult<T> = Result<T, ()>;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<Error>,
}

impl Parser {
    fn kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn kind_at(&self, offset: usize) -> &TokenKind {
        self.tokens
            .get(self.pos + offset)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn at_eof(&self) -> bool {
        matches!(self.kind(), TokenKind::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if !self.at_eof() {
            self.pos += 1;
        }
        token
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.kind() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.errors.push(Error::Parse(format!(
            "{} (at byte {})",
            message.into(),
            span.start
        )));
    }

    /// Reports that `expected` was wanted but the current token was found.
    fn error_expected(&mut self, expected: &str) {
        let found = self.kind().describe();
        let span = self.span();
        self.error(span, format!("expected {expected}, found {found}"));
    }

    fn expect(&mut self, kind: &TokenKind, expected: &str) -> PResult<Token> {
        if self.kind() == kind {
            Ok(self.bump())
        } else {
            self.error_expected(expected);
            Err(())
        }
    }

    fn parse_program(&mut self) -> Program {
        let mut statements = Vec::new();
        while !self.at_eof() {
            let start = self.pos;
            match self.parse_statement() {
                Ok(stmts) => statements.extend(stmts),
                Err(()) => self.recover_to_next_statement(),
            }
            // Guarantee forward progress even if a sub-parser consumed nothing.
            if self.pos == start && !self.at_eof() {
                self.bump();
            }
        }
        Program { statements }
    }

    /// Skips tokens up to and including the next `.`, so the next statement
    /// starts clean after an error.
    fn recover_to_next_statement(&mut self) {
        while !self.at_eof() {
            let is_dot = matches!(self.kind(), TokenKind::Dot);
            self.bump();
            if is_dot {
                return;
            }
        }
    }

    /// Parses one statement, which yields one [`Statement`] except a
    /// disjunctive rule, which expands to one per disjunct (decision 7).
    fn parse_statement(&mut self) -> PResult<Vec<Statement>> {
        match self.kind() {
            TokenKind::Import => Ok(vec![self.parse_import()?]),
            TokenKind::Declare => Ok(vec![self.parse_declare()?]),
            TokenKind::QuestionDash => Ok(vec![self.parse_query()?]),
            _ => self.parse_clause(),
        }
    }

    fn parse_import(&mut self) -> PResult<Statement> {
        let start = self.span();
        self.bump(); // `import`
        let path_tok = self.expect_string("a quoted path after `import`")?;
        let (path, path_span) = match path_tok.kind {
            TokenKind::Str(s) => (s, path_tok.span),
            _ => unreachable!("expect_string returns a string token"),
        };

        // Module form (§13): no `as` clause — `import "lib.dl".`
        if let TokenKind::Dot = self.kind() {
            let end = self.bump().span;
            return Ok(Statement {
                kind: StatementKind::Import(Import {
                    path,
                    path_span,
                    kind: ImportKind::Module,
                }),
                span: join(start, end),
            });
        }

        // `table "<name>"` selection — `table` is a contextual keyword (§5):
        // after the path only `.`, `table`, or `as` can follow, so an ident
        // spelled `table` here is unambiguous and stays an ordinary ident
        // everywhere else.
        let table = if matches!(self.kind(), TokenKind::Ident(name) if name == "table") {
            self.bump();
            let table_tok = self.expect_string("a quoted table name after `table`")?;
            match table_tok.kind {
                TokenKind::Str(s) => Some((s, table_tok.span)),
                _ => unreachable!("expect_string returns a string token"),
            }
        } else {
            None
        };

        self.expect(
            &TokenKind::As,
            "`as` after the import path (or `.` for a module import)",
        )?;
        let relation = self.expect_relation_name("a relation name after `as`")?;
        let schema = if self.eat(&TokenKind::LParen) {
            Some(self.parse_field_list()?)
        } else {
            None
        };
        let end = self.expect(&TokenKind::Dot, "`.` to end the import")?.span;
        Ok(Statement {
            kind: StatementKind::Import(Import {
                path,
                path_span,
                kind: ImportKind::Data {
                    table,
                    relation,
                    schema,
                },
            }),
            span: join(start, end),
        })
    }

    fn parse_declare(&mut self) -> PResult<Statement> {
        let start = self.span();
        self.bump(); // `declare`
        let relation = self.expect_relation_name("a relation name after `declare`")?;
        self.expect(&TokenKind::LParen, "`(` after the relation name")?;
        let fields = self.parse_field_list()?;
        let end = self
            .expect(&TokenKind::Dot, "`.` to end the declaration")?
            .span;
        Ok(Statement {
            kind: StatementKind::Declare(Declaration { relation, fields }),
            span: join(start, end),
        })
    }

    /// Parses `field { "," field } ")"`, consuming the closing paren.
    fn parse_field_list(&mut self) -> PResult<Vec<FieldDecl>> {
        let mut fields = Vec::new();
        if matches!(self.kind(), TokenKind::RParen) {
            self.error_expected("at least one field name");
            self.bump();
            return Err(());
        }
        loop {
            fields.push(self.parse_field()?);
            if self.eat(&TokenKind::Comma) {
                if matches!(self.kind(), TokenKind::RParen) {
                    let span = self.span();
                    self.error(span, "trailing `,` in the field list");
                    return Err(());
                }
                continue;
            }
            break;
        }
        self.expect(&TokenKind::RParen, "`)` to close the field list")?;
        Ok(fields)
    }

    fn parse_field(&mut self) -> PResult<FieldDecl> {
        let name = self.expect_lowercase_ident("a field name")?;
        let span_start = name.span;
        let (ty, end) = if self.eat(&TokenKind::Colon) {
            let (ty, span) = self.parse_type()?;
            (Some(ty), span)
        } else {
            (None, name.span)
        };
        Ok(FieldDecl {
            name,
            ty,
            span: join(span_start, end),
        })
    }

    /// The five type names are *contextual* — lexed as identifiers, recognized
    /// only here.
    fn parse_type(&mut self) -> PResult<(TypeName, Span)> {
        let span = self.span();
        if let TokenKind::Ident(name) = self.kind() {
            let ty = match name.as_str() {
                "int" => Some(TypeName::Int),
                "float" => Some(TypeName::Float),
                "string" => Some(TypeName::String),
                "symbol" => Some(TypeName::Symbol),
                "bool" => Some(TypeName::Bool),
                _ => None,
            };
            if let Some(ty) = ty {
                self.bump();
                return Ok((ty, span));
            }
        }
        self.error_expected("a type name (int, float, string, symbol, or bool)");
        Err(())
    }

    fn parse_query(&mut self) -> PResult<Statement> {
        let start = self.span();
        self.bump(); // `?-`
        let body = self.parse_conjunction()?;
        // Queries are conjunctive in v1 — a `;` here is disjunction, deferred.
        if matches!(self.kind(), TokenKind::Semi) {
            let span = self.span();
            self.error(
                span,
                "disjunction `;` is not supported in queries; split into separate queries",
            );
            return Err(());
        }
        let end = self.expect(&TokenKind::Dot, "`.` to end the query")?.span;
        Ok(Statement {
            kind: StatementKind::Query(Query {
                body,
                span: join(start, end),
            }),
            span: join(start, end),
        })
    }

    /// Parses a clause: a fact (no body) or a rule. A disjunctive rule body
    /// expands to one [`Clause`] per disjunct, all sharing the head.
    fn parse_clause(&mut self) -> PResult<Vec<Statement>> {
        let head = self.parse_atom()?;
        let start = head.span;
        match self.kind() {
            TokenKind::Dot => {
                let end = self.bump().span;
                Ok(vec![Statement {
                    kind: StatementKind::Clause(Clause {
                        head,
                        body: Vec::new(),
                        span: join(start, end),
                    }),
                    span: join(start, end),
                }])
            }
            TokenKind::ColonDash => {
                self.bump();
                let disjuncts = self.parse_body()?;
                let end = self.expect(&TokenKind::Dot, "`.` to end the rule")?.span;
                let span = join(start, end);
                Ok(disjuncts
                    .into_iter()
                    .map(|body| Statement {
                        kind: StatementKind::Clause(Clause {
                            head: head.clone(),
                            body,
                            span,
                        }),
                        span,
                    })
                    .collect())
            }
            _ => {
                self.error_expected("`.` for a fact or `:-` for a rule after the head");
                Err(())
            }
        }
    }

    /// Parses a rule body as a disjunction of conjunctions (top-level DNF, no
    /// parentheses in v1); `,` binds tighter than `;` (decision 7).
    fn parse_body(&mut self) -> PResult<Vec<Vec<Literal>>> {
        let mut disjuncts = vec![self.parse_conjunction()?];
        while self.eat(&TokenKind::Semi) {
            disjuncts.push(self.parse_conjunction()?);
        }
        Ok(disjuncts)
    }

    fn parse_conjunction(&mut self) -> PResult<Vec<Literal>> {
        let mut literals = vec![self.parse_literal()?];
        while self.eat(&TokenKind::Comma) {
            if matches!(self.kind(), TokenKind::Dot | TokenKind::Semi) {
                let span = self.span();
                self.error(span, "trailing `,` in the body");
                return Err(());
            }
            literals.push(self.parse_literal()?);
        }
        Ok(literals)
    }

    fn parse_literal(&mut self) -> PResult<Literal> {
        if matches!(self.kind(), TokenKind::Not) {
            let start = self.span();
            self.bump();
            // `not` applies to atoms only (§5). An atom is `ident ( … )`.
            if !(matches!(self.kind(), TokenKind::Ident(_))
                && matches!(self.kind_at(1), TokenKind::LParen))
            {
                self.error_expected(
                    "an atom after `not` (negation applies to atoms, not comparisons)",
                );
                return Err(());
            }
            let atom = self.parse_atom()?;
            let span = join(start, atom.span);
            return Ok(Literal {
                kind: LiteralKind::Atom {
                    negated: true,
                    atom,
                },
                span,
            });
        }

        // A positive atom is `ident (`; anything else begins a comparison.
        if matches!(self.kind(), TokenKind::Ident(_))
            && matches!(self.kind_at(1), TokenKind::LParen)
        {
            let atom = self.parse_atom()?;
            let span = atom.span;
            return Ok(Literal {
                kind: LiteralKind::Atom {
                    negated: false,
                    atom,
                },
                span,
            });
        }

        let (kind, span) = self.parse_comparison_or_presence()?;
        Ok(Literal { kind, span })
    }

    /// A comparison `expr cmp expr` or a presence test `expr is [not] absent`
    /// (§4/§8) — the two share a left operand, so they are parsed together.
    fn parse_comparison_or_presence(&mut self) -> PResult<(LiteralKind, Span)> {
        let lhs = self.parse_expr()?;

        // Presence test: `expr is [not] absent`. The `not` is part of the
        // operator, not §5 atom-negation.
        if matches!(self.kind(), TokenKind::Is) {
            self.bump();
            let negated = matches!(self.kind(), TokenKind::Not);
            if negated {
                self.bump();
            }
            let absent_span = self.span();
            if !matches!(self.kind(), TokenKind::Absent) {
                self.error_expected(
                    "`absent` (the only presence test is `is absent` / `is not absent`)",
                );
                return Err(());
            }
            self.bump();
            let span = join(lhs.span, absent_span);
            return Ok((LiteralKind::Presence { expr: lhs, negated }, span));
        }

        let Some(op) = self.comparison_op() else {
            self.error_expected("a comparison operator (=, !=, <, <=, >, >=) or `is [not] absent`");
            return Err(());
        };
        self.bump();
        let rhs = self.parse_expr()?;
        // Comparisons do not chain (decision 2), and a presence test cannot be
        // chained onto a comparison either.
        if self.comparison_op().is_some() || matches!(self.kind(), TokenKind::Is) {
            let span = self.span();
            self.error(
                span,
                "comparisons do not chain; write `a <= b, b <= c` instead of `a <= b <= c`",
            );
            return Err(());
        }
        let span = join(lhs.span, rhs.span);
        Ok((LiteralKind::Comparison(Comparison { op, lhs, rhs }), span))
    }

    fn comparison_op(&self) -> Option<crate::ast::CmpOp> {
        use crate::ast::CmpOp;
        Some(match self.kind() {
            TokenKind::Eq => CmpOp::Eq,
            TokenKind::Ne => CmpOp::Ne,
            TokenKind::Lt => CmpOp::Lt,
            TokenKind::Le => CmpOp::Le,
            TokenKind::Gt => CmpOp::Gt,
            TokenKind::Ge => CmpOp::Ge,
            _ => return None,
        })
    }

    // --- Expressions (precedence climbing) ---

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> PResult<Expr> {
        use crate::ast::ArithOp;
        let mut lhs = self.parse_multiplicative()?;
        loop {
            let op = match self.kind() {
                TokenKind::Plus => ArithOp::Add,
                TokenKind::Minus => ArithOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_multiplicative()?;
            lhs = fold_binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> PResult<Expr> {
        use crate::ast::ArithOp;
        let mut lhs = self.parse_primary()?;
        loop {
            let op = match self.kind() {
                TokenKind::Star => ArithOp::Mul,
                TokenKind::Slash => ArithOp::Div,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_primary()?;
            lhs = fold_binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    /// A primary expression: a term, or a prefix `-` folded onto a numeric
    /// literal (decision 3).
    fn parse_primary(&mut self) -> PResult<Expr> {
        if matches!(self.kind(), TokenKind::Minus) {
            let start = self.span();
            self.bump();
            let span = self.span();
            let constant = match self.kind() {
                TokenKind::Int(n) => {
                    let n = *n;
                    self.bump();
                    Constant::Int(-n)
                }
                TokenKind::Float(f) => {
                    let f = *f;
                    self.bump();
                    Constant::Float(-f)
                }
                _ => {
                    self.error(
                        start,
                        "prefix `-` applies only to a numeric literal (there is no unary minus on \
                         variables or expressions)",
                    );
                    return Err(());
                }
            };
            let span = join(start, span);
            return Ok(Expr {
                kind: ExprKind::Term(Term {
                    kind: TermKind::Constant(constant),
                    span,
                }),
                span,
            });
        }
        // A set-builder aggregate `op { expr | goal }` (§9). Contextual dispatch:
        // an operator name matters only immediately before `{`; everywhere else
        // `count`/`sum`/… stay ordinary identifiers (symbols, relation and field
        // names). `{` never appears outside an aggregate, so an identifier before
        // it that is not one of the five is a targeted error, not a fall-through.
        if let TokenKind::Ident(name) = self.kind()
            && matches!(self.kind_at(1), TokenKind::LBrace)
        {
            if let Some(op) = AggOp::from_name(name) {
                return self.parse_aggregate(op);
            }
            let span = self.span();
            self.error(
                span,
                format!(
                    "`{name}` is not an aggregate operator; expected one of \
                     count, sum, min, max, avg before `{{`"
                ),
            );
            return Err(());
        }
        if matches!(self.kind(), TokenKind::LBrace) {
            let span = self.span();
            self.error(
                span,
                "an aggregate needs an operator: write `count { X | goal(X) }` \
                 (one of count, sum, min, max, avg) before the `{`",
            );
            return Err(());
        }
        // Grouping parens are not in the v1 grammar (`primary = [ "-" ] number |
        // term`, §5). Catch them here with the decomposition workaround rather
        // than letting `parse_term` report a bare "expected a term" — the reader
        // is often an agent, and the actionable form is self-healing.
        if matches!(self.kind(), TokenKind::LParen) {
            let span = self.span();
            self.error(
                span,
                "parentheses are not supported in expressions; introduce an intermediate \
                 variable instead (e.g. `(A + B) * C` becomes `T = A + B, X = T * C`)",
            );
            return Err(());
        }
        let term = self.parse_term()?;
        let span = term.span;
        Ok(Expr {
            kind: ExprKind::Term(term),
            span,
        })
    }

    /// A set-builder aggregate `op { expr | goal }` (§9). The operator has been
    /// identified from the identifier immediately before `{` (contextual
    /// dispatch, `parse_primary`) but neither token is consumed yet. The goal is
    /// a conjunction — disjunction is not allowed inside an aggregate (§5).
    fn parse_aggregate(&mut self, op: AggOp) -> PResult<Expr> {
        let start = self.span();
        self.bump(); // the operator identifier
        self.expect(&TokenKind::LBrace, "`{` opening the aggregate")?;
        let expr = self.parse_expr()?;
        if !self.eat(&TokenKind::Pipe) {
            self.error_expected(
                "`|` separating the aggregated expression from its goal \
                 (`op { Expr | Goal }`)",
            );
            return Err(());
        }
        let goal = self.parse_conjunction()?;
        let close = self.expect(&TokenKind::RBrace, "`}` closing the aggregate")?;
        let span = join(start, close.span);
        Ok(Expr {
            kind: ExprKind::Aggregate(Aggregate {
                op,
                expr: Box::new(expr),
                goal,
                params: Vec::new(),
            }),
            span,
        })
    }

    fn parse_term(&mut self) -> PResult<Term> {
        let span = self.span();
        let kind = match self.kind() {
            TokenKind::Int(n) => {
                let c = Constant::Int(*n);
                self.bump();
                TermKind::Constant(c)
            }
            TokenKind::Float(f) => {
                let c = Constant::Float(*f);
                self.bump();
                TermKind::Constant(c)
            }
            TokenKind::Str(s) => {
                let c = Constant::String(s.clone());
                self.bump();
                TermKind::Constant(c)
            }
            TokenKind::True => {
                self.bump();
                TermKind::Constant(Constant::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                TermKind::Constant(Constant::Bool(false))
            }
            TokenKind::Absent => {
                self.bump();
                TermKind::Constant(Constant::Absent)
            }
            TokenKind::Ident(name) => {
                // A bare identifier is a symbol constant. `ident (` would be a
                // compound term, which v1 forbids (§4, flat terms).
                if matches!(self.kind_at(1), TokenKind::LParen) {
                    self.error(
                        span,
                        "compound terms are not supported; arguments are flat (§4)",
                    );
                    return Err(());
                }
                let c = Constant::Symbol(name.clone());
                self.bump();
                TermKind::Constant(c)
            }
            TokenKind::Variable(name) => {
                let kind = if name == "_" {
                    TermKind::Wildcard
                } else {
                    TermKind::Variable(name.clone())
                };
                self.bump();
                kind
            }
            _ => {
                self.error_expected("a term (a constant or a variable)");
                return Err(());
            }
        };
        Ok(Term { kind, span })
    }

    // --- Atoms ---

    fn parse_atom(&mut self) -> PResult<Atom> {
        let predicate = self.expect_relation_name("a relation name")?;
        let start = predicate.span;
        self.expect(&TokenKind::LParen, "`(` after the relation name")?;

        if matches!(self.kind(), TokenKind::RParen) {
            let span = self.span();
            self.error(
                span,
                format!(
                    "predicate `{}` has no arguments; predicates take at least one argument in v1",
                    predicate.name
                ),
            );
            return Err(());
        }

        // Named iff the first argument is `ident :` (the `:` token, never the
        // `:-` rule arrow, which lexes distinctly).
        let named = matches!(self.kind(), TokenKind::Ident(_))
            && matches!(self.kind_at(1), TokenKind::Colon);
        let args = if named {
            Args::Named(self.parse_named_args()?)
        } else {
            Args::Positional(self.parse_positional_args()?)
        };

        let end = self.expect(&TokenKind::RParen, "`)` to close the argument list")?;
        Ok(Atom {
            predicate,
            args,
            span: join(start, end.span),
        })
    }

    fn parse_positional_args(&mut self) -> PResult<Vec<Expr>> {
        let mut args = Vec::new();
        loop {
            // Detect a named argument appearing among positional ones.
            if matches!(self.kind(), TokenKind::Ident(_))
                && matches!(self.kind_at(1), TokenKind::Colon)
            {
                let span = self.span();
                self.error(
                    span,
                    "cannot mix positional and named arguments in one literal (§4)",
                );
                return Err(());
            }
            args.push(self.parse_expr()?);
            if self.eat(&TokenKind::Comma) {
                if matches!(self.kind(), TokenKind::RParen) {
                    let span = self.span();
                    self.error(span, "trailing `,` in the argument list");
                    return Err(());
                }
                continue;
            }
            break;
        }
        Ok(args)
    }

    fn parse_named_args(&mut self) -> PResult<Vec<NamedArg>> {
        let mut args = Vec::new();
        loop {
            if !(matches!(self.kind(), TokenKind::Ident(_))
                && matches!(self.kind_at(1), TokenKind::Colon))
            {
                self.error_expected(
                    "a named argument `field: value` (a literal is all-named or all-positional, §4)",
                );
                return Err(());
            }
            let field = self.expect_lowercase_ident("a field name")?;
            self.expect(&TokenKind::Colon, "`:` after the field name")?;
            let value = self.parse_expr()?;
            let span = join(field.span, value.span);
            args.push(NamedArg { field, value, span });
            if self.eat(&TokenKind::Comma) {
                if matches!(self.kind(), TokenKind::RParen) {
                    let span = self.span();
                    self.error(span, "trailing `,` in the argument list");
                    return Err(());
                }
                continue;
            }
            break;
        }
        Ok(args)
    }

    // --- Token helpers with casing-aware errors ---

    fn expect_string(&mut self, expected: &str) -> PResult<Token> {
        if matches!(self.kind(), TokenKind::Str(_)) {
            Ok(self.bump())
        } else {
            self.error_expected(expected);
            Err(())
        }
    }

    /// A relation name is a lowercase identifier. A [`TokenKind::Variable`]
    /// here is the "capitalized relation" near-miss — report it with the strict
    /// Prolog-casing hint but keep the spelling so parsing recovers.
    fn expect_relation_name(&mut self, expected: &str) -> PResult<Ident> {
        match self.kind().clone() {
            TokenKind::Ident(name) => {
                let span = self.span();
                self.bump();
                Ok(Ident { name, span })
            }
            TokenKind::Variable(name) => {
                let span = self.span();
                self.error(
                    span,
                    format!(
                        "relation names must be lowercase; `{name}` looks like a variable \
                         (did you mean `{}`?)",
                        lowercase_first(&name)
                    ),
                );
                self.bump();
                Ok(Ident { name, span })
            }
            _ => {
                self.error_expected(expected);
                Err(())
            }
        }
    }

    fn expect_lowercase_ident(&mut self, expected: &str) -> PResult<Ident> {
        if let TokenKind::Ident(name) = self.kind().clone() {
            let span = self.span();
            self.bump();
            Ok(Ident { name, span })
        } else {
            self.error_expected(expected);
            Err(())
        }
    }
}

/// Left-associative binary folding used by precedence climbing.
fn fold_binary(op: crate::ast::ArithOp, lhs: Expr, rhs: Expr) -> Expr {
    let span = join(lhs.span, rhs.span);
    Expr {
        kind: ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        span,
    }
}

/// The span covering both `a` and `b` (they arrive in source order).
fn join(a: Span, b: Span) -> Span {
    Span {
        start: a.start.min(b.start),
        end: a.end.max(b.end),
    }
}

fn lowercase_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::fixtures;

    /// Zeroes every span in a program so parsed output (real spans) can be
    /// compared structurally against the hand-built fixtures (which use
    /// [`Span::DUMMY`]).
    fn zero_spans(program: &mut Program) {
        for statement in &mut program.statements {
            statement.span = Span::DUMMY;
            match &mut statement.kind {
                StatementKind::Import(import) => {
                    import.path_span = Span::DUMMY;
                    match &mut import.kind {
                        ImportKind::Module => {}
                        ImportKind::Data {
                            table,
                            relation,
                            schema,
                        } => {
                            if let Some((_, table_span)) = table {
                                *table_span = Span::DUMMY;
                            }
                            relation.span = Span::DUMMY;
                            if let Some(schema) = schema {
                                schema.iter_mut().for_each(zero_field);
                            }
                        }
                    }
                }
                StatementKind::Declare(declaration) => {
                    declaration.relation.span = Span::DUMMY;
                    declaration.fields.iter_mut().for_each(zero_field);
                }
                StatementKind::Clause(clause) => {
                    clause.span = Span::DUMMY;
                    zero_atom(&mut clause.head);
                    clause.body.iter_mut().for_each(zero_literal);
                }
                StatementKind::Query(query) => {
                    query.span = Span::DUMMY;
                    query.body.iter_mut().for_each(zero_literal);
                }
            }
        }
    }

    fn zero_field(field: &mut FieldDecl) {
        field.name.span = Span::DUMMY;
        field.span = Span::DUMMY;
    }

    fn zero_atom(atom: &mut Atom) {
        atom.predicate.span = Span::DUMMY;
        atom.span = Span::DUMMY;
        match &mut atom.args {
            Args::Positional(exprs) => exprs.iter_mut().for_each(zero_expr),
            Args::Named(named) => {
                for arg in named {
                    arg.field.span = Span::DUMMY;
                    arg.span = Span::DUMMY;
                    zero_expr(&mut arg.value);
                }
            }
        }
    }

    fn zero_literal(literal: &mut Literal) {
        literal.span = Span::DUMMY;
        match &mut literal.kind {
            LiteralKind::Atom { atom, .. } => zero_atom(atom),
            LiteralKind::Comparison(cmp) => {
                zero_expr(&mut cmp.lhs);
                zero_expr(&mut cmp.rhs);
            }
            LiteralKind::Presence { expr, .. } => zero_expr(expr),
        }
    }

    fn zero_expr(expr: &mut Expr) {
        expr.span = Span::DUMMY;
        match &mut expr.kind {
            ExprKind::Term(term) => term.span = Span::DUMMY,
            ExprKind::Binary { lhs, rhs, .. } => {
                zero_expr(lhs);
                zero_expr(rhs);
            }
            ExprKind::Aggregate(agg) => {
                zero_expr(&mut agg.expr);
                for literal in &mut agg.goal {
                    zero_literal(literal);
                }
                for param in &mut agg.params {
                    zero_expr(param);
                }
            }
        }
    }

    fn parse_ok(src: &str) -> Program {
        let mut program = parse(src).unwrap_or_else(|e| panic!("parse failed: {e:?}"));
        zero_spans(&mut program);
        program
    }

    fn parse_err(src: &str) -> Vec<Error> {
        parse(src).expect_err("expected a parse error")
    }

    // --- Golden AST fixtures: the §16 corpus (spec §16 source text) ---

    #[test]
    fn golden_16_1_ancestry() {
        let src = "\
parent(\"alice\", \"bob\").
parent(\"bob\", \"carol\").
parent(\"carol\", \"dave\").
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
?- ancestor(\"alice\", Who).
";
        assert_eq!(parse_ok(src), fixtures::example_16_1());
    }

    #[test]
    fn golden_16_2_negation() {
        let src = "\
person(\"alice\").
person(\"bob\").
person(\"carol\").
parent(\"alice\", \"bob\").
parent(\"bob\", \"carol\").
root(X) :- person(X), not parent(_, X).
";
        assert_eq!(parse_ok(src), fixtures::example_16_2());
    }

    #[test]
    fn golden_16_7_named_arguments() {
        let src = "\
import \"data/employees.csv\" as employee(id: int, name: string, age: int, dept: string, title: string, salary: int, city: string, start_date: string).
manager_name(N) :- employee(name: N, title: \"manager\").
declare person(name: string, age: int).
person(\"alice\", 30).
adult(N) :- person(name: N, age: A), A >= 18.
";
        assert_eq!(parse_ok(src), fixtures::example_16_7());
    }

    // --- Import forms (§13) ---

    #[test]
    fn module_import_parses_without_as() {
        let program = parse_ok("import \"lib/family.dl\".");
        let StatementKind::Import(import) = &program.statements[0].kind else {
            panic!("expected an import");
        };
        assert_eq!(import.path, "lib/family.dl");
        assert_eq!(import.kind, ImportKind::Module);
    }

    #[test]
    fn table_import_parses_with_selection() {
        let program = parse_ok("import \"analytics.duckdb\" table \"orders\" as order.");
        let StatementKind::Import(import) = &program.statements[0].kind else {
            panic!("expected an import");
        };
        assert_eq!(import.path, "analytics.duckdb");
        let ImportKind::Data {
            table,
            relation,
            schema,
        } = &import.kind
        else {
            panic!("expected a data import");
        };
        assert_eq!(
            table.as_ref().map(|(name, _)| name.as_str()),
            Some("orders")
        );
        assert_eq!(relation.name, "order");
        assert!(schema.is_none());
    }

    #[test]
    fn table_import_composes_with_an_explicit_schema() {
        let program = parse_ok("import \"db.sqlite\" table \"t\" as t(a: int, b: string).");
        let StatementKind::Import(import) = &program.statements[0].kind else {
            panic!("expected an import");
        };
        let ImportKind::Data { schema, .. } = &import.kind else {
            panic!("expected a data import");
        };
        assert_eq!(schema.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn table_stays_an_ordinary_identifier_elsewhere() {
        // `table` is contextual (§5): fine as a relation name, a field name,
        // and a symbol constant.
        let program = parse_ok("declare table(kind: symbol).\ntable(dining).\np(X) :- table(X).");
        assert_eq!(program.statements.len(), 3);
        let StatementKind::Declare(declaration) = &program.statements[0].kind else {
            panic!("expected a declare");
        };
        assert_eq!(declaration.relation.name, "table");
    }

    #[test]
    fn import_error_mentions_the_module_alternative() {
        let errors = parse_err("import \"x.csv\" garbage.");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("or `.` for a module import")),
            "got: {errors:?}"
        );
    }

    // --- Core grammar ---

    #[test]
    fn comment_markers_and_whitespace_are_ignored() {
        let a = parse_ok("% a comment\np(1).\n# another\n");
        let b = parse_ok("p(1).");
        assert_eq!(a, b);
    }

    #[test]
    fn signed_literals_fold_to_negative_constants() {
        let program = parse_ok("p(-7, -0.5).");
        let StatementKind::Clause(clause) = &program.statements[0].kind else {
            panic!("expected a clause");
        };
        let Args::Positional(args) = &clause.head.args else {
            panic!("positional");
        };
        assert_eq!(
            args[0].kind,
            ExprKind::Term(Term {
                kind: TermKind::Constant(Constant::Int(-7)),
                span: Span::DUMMY,
            })
        );
        assert_eq!(
            args[1].kind,
            ExprKind::Term(Term {
                kind: TermKind::Constant(Constant::Float(-0.5)),
                span: Span::DUMMY,
            })
        );
    }

    #[test]
    fn bare_identifier_is_a_symbol_constant() {
        let program = parse_ok("color(red).");
        let StatementKind::Clause(clause) = &program.statements[0].kind else {
            panic!("clause");
        };
        let Args::Positional(args) = &clause.head.args else {
            panic!("positional");
        };
        assert_eq!(
            args[0].kind,
            ExprKind::Term(Term {
                kind: TermKind::Constant(Constant::Symbol("red".into())),
                span: Span::DUMMY,
            })
        );
    }

    #[test]
    fn presence_test_parses_both_polarities() {
        for (src, want_negated) in [
            ("p(X) :- q(X), X is absent.", false),
            ("p(X) :- q(X), X is not absent.", true),
        ] {
            let program = parse_ok(src);
            let StatementKind::Clause(clause) = &program.statements[0].kind else {
                panic!("clause");
            };
            let LiteralKind::Presence { expr, negated } = &clause.body[1].kind else {
                panic!("expected a presence test, got {:?}", clause.body[1].kind);
            };
            assert_eq!(*negated, want_negated);
            assert!(matches!(
                expr.kind,
                ExprKind::Term(Term {
                    kind: TermKind::Variable(_),
                    ..
                })
            ));
        }
    }

    #[test]
    fn absent_parses_as_a_constant_in_term_position() {
        let program = parse_ok("p(absent).");
        let StatementKind::Clause(clause) = &program.statements[0].kind else {
            panic!("clause");
        };
        let Args::Positional(args) = &clause.head.args else {
            panic!("positional");
        };
        assert_eq!(
            args[0].kind,
            ExprKind::Term(Term {
                kind: TermKind::Constant(Constant::Absent),
                span: Span::DUMMY,
            })
        );
    }

    #[test]
    fn is_without_absent_is_a_targeted_error() {
        let errors = parse_err("p(X) :- q(X), X is 5.");
        assert!(errors[0].to_string().contains("absent"), "got: {errors:?}");
    }

    #[test]
    fn absent_is_reserved_and_cannot_name_a_relation() {
        // A keyword, not an identifier — `absent(...)` cannot be a head atom.
        assert!(!parse_err("absent(1).").is_empty());
    }

    #[test]
    fn aggregate_parses_with_the_pipe_separator() {
        // `N = count { C | parent(P, C) }` (§9): the RHS is an aggregate whose
        // collected expression is `C` and whose goal is one positive atom.
        let program =
            parse_ok("child_count(P, N) :- parent(P, _), N = count { C | parent(P, C) }.");
        let StatementKind::Clause(clause) = &program.statements[0].kind else {
            panic!("clause");
        };
        let LiteralKind::Comparison(cmp) = &clause.body[1].kind else {
            panic!("comparison");
        };
        let ExprKind::Aggregate(agg) = &cmp.rhs.kind else {
            panic!("aggregate on the rhs");
        };
        assert_eq!(agg.op, AggOp::Count);
        assert!(
            matches!(&agg.expr.kind, ExprKind::Term(Term { kind: TermKind::Variable(name), .. }) if name == "C"),
            "collected expression is the variable C"
        );
        assert_eq!(agg.goal.len(), 1);
        assert!(matches!(
            &agg.goal[0].kind,
            LiteralKind::Atom { negated: false, .. }
        ));
        assert!(agg.params.is_empty(), "no parameters in the v1 five");
    }

    #[test]
    fn aggregate_operator_names_stay_usable_as_relations() {
        // Contextual dispatch (§9): `count`/… are aggregate operators only
        // immediately before `{`; elsewhere they are ordinary identifiers.
        let program = parse_ok("count(5).\n?- count(X).");
        let StatementKind::Clause(clause) = &program.statements[0].kind else {
            panic!("clause");
        };
        assert_eq!(clause.head.predicate.name, "count");
    }

    #[test]
    fn aggregate_without_a_pipe_is_a_targeted_error() {
        let errors = parse_err("p(N) :- N = count { C , parent(P, C) }.");
        assert!(errors[0].to_string().contains('|'), "got: {errors:?}");
    }

    #[test]
    fn a_non_operator_before_a_brace_is_a_targeted_error() {
        let errors = parse_err("p(N) :- N = blah { C | q(C) }.");
        assert!(
            errors[0].to_string().contains("aggregate operator"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn arithmetic_precedence_is_mul_over_add_left_assoc() {
        // 1 + 2 * 3 - 4  ==  (1 + (2*3)) - 4
        let program = parse_ok("p(X) :- X = 1 + 2 * 3 - 4.");
        let StatementKind::Clause(clause) = &program.statements[0].kind else {
            panic!("clause");
        };
        let LiteralKind::Comparison(cmp) = &clause.body[0].kind else {
            panic!("comparison");
        };
        // Expected rhs: Sub(Add(1, Mul(2,3)), 4)
        use crate::ast::ArithOp;
        let ExprKind::Binary {
            op: ArithOp::Sub,
            lhs,
            rhs,
        } = &cmp.rhs.kind
        else {
            panic!("outermost is subtraction, got {:?}", cmp.rhs.kind);
        };
        assert!(matches!(rhs.kind, ExprKind::Term(_)), "rhs of Sub is 4");
        let ExprKind::Binary {
            op: ArithOp::Add,
            rhs: add_rhs,
            ..
        } = &lhs.kind
        else {
            panic!("lhs is an addition");
        };
        assert!(matches!(
            add_rhs.kind,
            ExprKind::Binary {
                op: ArithOp::Mul,
                ..
            }
        ));
    }

    #[test]
    fn disjunction_expands_to_multiple_clauses() {
        let program = parse_ok("p(X) :- a(X) ; b(X).");
        assert_eq!(program.statements.len(), 2);
        for (i, pred) in ["a", "b"].iter().enumerate() {
            let StatementKind::Clause(clause) = &program.statements[i].kind else {
                panic!("clause");
            };
            assert_eq!(clause.head.predicate.name, "p");
            let LiteralKind::Atom { atom, .. } = &clause.body[0].kind else {
                panic!("atom");
            };
            assert_eq!(&atom.predicate.name, pred);
        }
    }

    #[test]
    fn conjunction_binds_tighter_than_disjunction() {
        // a, b ; c  ==  (a, b) ; (c)
        let program = parse_ok("p(X) :- a(X), b(X) ; c(X).");
        assert_eq!(program.statements.len(), 2);
        let StatementKind::Clause(first) = &program.statements[0].kind else {
            panic!("clause");
        };
        assert_eq!(
            first.body.len(),
            2,
            "first disjunct is the conjunction a, b"
        );
        let StatementKind::Clause(second) = &program.statements[1].kind else {
            panic!("clause");
        };
        assert_eq!(second.body.len(), 1, "second disjunct is c");
    }

    #[test]
    fn inline_arithmetic_in_atom_args_parses() {
        // succ(N, N+1) :- number(N).  — the argument is a Binary expr.
        let program = parse_ok("succ(N, N + 1) :- number(N).");
        let StatementKind::Clause(clause) = &program.statements[0].kind else {
            panic!("clause");
        };
        let Args::Positional(args) = &clause.head.args else {
            panic!("positional");
        };
        assert!(
            matches!(args[1].kind, ExprKind::Binary { .. }),
            "second head arg is an inline expression"
        );
    }

    #[test]
    fn named_and_positional_facts_both_parse() {
        assert!(parse("person(\"alice\", 30).").is_ok());
        assert!(
            parse("declare person(name: string, age: int).\nperson(name: \"alice\", age: 30).")
                .is_ok()
        );
    }

    // --- Structured errors (did-you-mean set) ---

    fn asserts_message(src: &str, needle: &str) {
        let errors = parse_err(src);
        assert!(
            errors.iter().any(|e| e.to_string().contains(needle)),
            "for {src:?} expected a message containing {needle:?}, got {errors:?}"
        );
    }

    #[test]
    fn uppercase_relation_name_is_reported_with_a_hint() {
        asserts_message(
            "Parent(X, Y) :- edge(X, Y).",
            "relation names must be lowercase",
        );
    }

    #[test]
    fn missing_terminator_is_reported() {
        asserts_message("p(1)", "`.`");
    }

    #[test]
    fn zero_arity_atom_is_rejected() {
        asserts_message("p().", "at least one argument");
    }

    #[test]
    fn mixed_argument_styles_are_rejected() {
        asserts_message("p(1, name: X).", "mix positional and named");
    }

    #[test]
    fn not_before_a_comparison_is_rejected() {
        asserts_message("p(X) :- q(X), not X < 3.", "negation applies to atoms");
    }

    #[test]
    fn chained_comparison_is_rejected() {
        asserts_message("p(X) :- q(X), 0 <= X <= 9.", "comparisons do not chain");
    }

    #[test]
    fn trailing_comma_is_rejected() {
        asserts_message("p(1, 2,).", "trailing `,`");
    }

    #[test]
    fn compound_term_is_rejected() {
        asserts_message("p(f(1)).", "compound terms are not supported");
    }

    #[test]
    fn grouped_expression_is_rejected_with_the_decomposition_hint() {
        // Grouping parens aren't in the v1 expression grammar (`primary =
        // [ "-" ] number | term`, §5). Both entry paths — a comparison operand
        // and an inline atom argument — must surface the actionable hint, not a
        // bare "expected a term".
        asserts_message(
            "r(X) :- n(Y), X = (Y + 1) * 2.",
            "parentheses are not supported in expressions",
        );
        asserts_message("r(X) :- n(Y), X = (Y + 1) * 2.", "T = A + B, X = T * C");
        asserts_message(
            "double(N, (N + N)) :- n(N).",
            "parentheses are not supported",
        );
    }

    #[test]
    fn prefix_minus_on_a_variable_is_rejected() {
        asserts_message(
            "p(X) :- Y = -X, q(Y).",
            "prefix `-` applies only to a numeric literal",
        );
    }

    #[test]
    fn recovery_collects_multiple_errors_in_one_run() {
        // Two broken statements plus a good one — both errors reported.
        let errors = parse_err("Parent(X).\np(1)\nq(2).");
        assert!(
            errors.len() >= 2,
            "expected multiple errors, got {errors:?}"
        );
    }

    #[test]
    fn operator_near_misses_from_the_lexer_surface_through_parse() {
        asserts_message("p(X) :- q(X), X =< 3.", "did you mean `<=`?");
    }

    /// Phase D properties D1–D4 (testing.md).
    mod properties {
        use std::collections::BTreeSet;

        use proptest::prelude::*;

        use super::super::parse;
        use super::zero_spans;
        use crate::ir;
        use crate::lower::lower;
        use crate::print::{print_ground_fact, print_program};
        use crate::testgen::{arb_ast_program, arb_printable_fact_set};

        proptest! {
            /// D1 — the §14 closure: printing any IR fact set in canonical
            /// output form, then parsing and lowering, recovers the identical
            /// fact set. Datalog-out is Datalog-in.
            #[test]
            fn d1_fact_set_closure(facts in arb_printable_fact_set()) {
                let text: String = facts
                    .iter()
                    .map(|(pred, values)| print_ground_fact(pred, values) + "\n")
                    .collect();
                let program = parse(&text).expect("printed facts parse");
                let lowered = lower(&program).expect("printed facts lower");

                let got: BTreeSet<(String, Vec<ir::Value>)> = lowered
                    .facts
                    .iter()
                    .map(|f| (lowered.pred_info(f.pred).name.clone(), f.tuple.0.clone()))
                    .collect();
                // Set semantics: duplicate input facts collapse, so compare sets.
                let want: BTreeSet<(String, Vec<ir::Value>)> = facts.into_iter().collect();
                prop_assert_eq!(got, want);
            }

            /// D2 — `parse(print(ast)) == ast` (modulo spans) over generated
            /// parse-reachable ASTs, including named-argument and arithmetic
            /// forms.
            #[test]
            fn d2_ast_round_trip(program in arb_ast_program()) {
                let text = print_program(&program);
                let mut reparsed = parse(&text)
                    .unwrap_or_else(|e| panic!("printed AST failed to parse:\n{text}\n{e:?}"));
                let mut expected = program;
                zero_spans(&mut expected);
                zero_spans(&mut reparsed);
                prop_assert_eq!(reparsed, expected);
            }

            /// D3 — canonical fixpoint: printing is stable under a parse round
            /// trip, so the printed form is a genuine canonical representative.
            #[test]
            fn d3_canonical_print_is_a_fixpoint(program in arb_ast_program()) {
                let text = print_program(&program);
                let reparsed = parse(&text).expect("printed AST parses");
                prop_assert_eq!(print_program(&reparsed), text);
            }

            /// D4 — `parse` never panics on arbitrary text …
            #[test]
            fn d4_parse_never_panics_on_text(src in ".*") {
                let _ = parse(&src);
            }

            /// … nor on arbitrary bytes (via lossy UTF-8 decoding).
            #[test]
            fn d4_parse_never_panics_on_bytes(
                bytes in proptest::collection::vec(any::<u8>(), 0..64),
            ) {
                let src = String::from_utf8_lossy(&bytes);
                let _ = parse(&src);
            }
        }
    }
}
