//! Crate-wide error types (`spec.md` §12).
//!
//! Errors are a design pillar: they must be *structured* and *actionable* so an
//! LLM/agent can react to them programmatically rather than scraping text.
//!
//! [`Error`] is therefore **data with a rendering**, not a rendering with data
//! attached (§12, 2026-07-25). It carries its category, its sentence, and —
//! where the producing stage knew one — a source span resolved to a 1-based
//! **line and column**. Before this, every error was a bare `String` with the
//! location `format!`ed into it as `"(at byte 217)"`: nothing downstream could
//! recover a position without parsing English, and a byte offset cannot be
//! turned into a caret or a "file:line" an editor will jump to.
//!
//! A span is attached by whichever stage knows one. The lexer and parser hold
//! the program text and resolve as they go ([`Error::at`]); every later stage
//! works over the IR and does not, so it records the span alone
//! ([`Error::at_span`]) and the position is resolved once at the API boundary
//! ([`locate_all`]). A program that spliced in a module has no single text to
//! resolve against, and those spans are dropped rather than mis-rendered
//! ([`forget_spans`]).
//!
//! Every error also carries an [`ErrorCode`]: a stable kebab-case name for what
//! kind of thing is wrong, so a consumer branches on the code and never on the
//! sentence. The category says which stage rejected the program; the code says
//! what the author has to change.
//!
//! Still to come (§12, tracked in `ROADMAP.md`): per-file attribution, which is
//! what would let a multi-file program keep its positions.

use std::fmt;

use crate::ast::Span;

/// Convenient result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Which stage rejected the program, and so which vocabulary the message speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A lexical error (bad token, unterminated literal, …).
    Lex,
    /// A syntax error (grammar violation).
    Parse,
    /// A semantic/safety error (unsafe rule, stratification violation, type
    /// conflict, …).
    Semantic,
    /// An error while loading facts from an external source (§13).
    Source,
}

impl ErrorKind {
    /// The label this kind is rendered under.
    pub fn label(self) -> &'static str {
        match self {
            ErrorKind::Lex => "lexical error",
            ErrorKind::Parse => "syntax error",
            ErrorKind::Semantic => "semantic error",
            ErrorKind::Source => "source error",
        }
    }
}

/// **What kind of thing is wrong**, as a stable name a consumer can branch on
/// (§12).
///
/// The category ([`ErrorKind`]) says which stage rejected the program, which is
/// a fact about the engine; this says what the program's author has to change.
/// A code belongs to exactly one category — [`ErrorCode::kind`] is the single
/// place that pairing is stated — so knowing the code implies the category and
/// never the reverse.
///
/// **A code exists where a *fix* differs in kind**, not where a message differs:
/// which field is unknown is what the message is for, and *the named arguments
/// do not match the schema* is what [`FieldMismatch`](ErrorCode::FieldMismatch)
/// is for. The census the vocabulary was derived from, and the folds that shrank
/// it, are in `notes/error-codes.md`.
///
/// **Stable and additive** (§12): a code is never repurposed, adding one is a
/// compatible change, and a consumer that does not recognise one falls back to
/// the category — which is why [`ALL`](ErrorCode::ALL) is pinned by a test.
/// Adding a variant is enforced in two of the three places by the compiler (the
/// exhaustive matches in [`as_str`](ErrorCode::as_str) and
/// [`kind`](ErrorCode::kind)); the third, `ALL`, is the one to remember, and the
/// pin is where a reviewer sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    // ---- lex ----
    /// A token spelled the way another language spells it — `=<`, `!`, `\=`,
    /// `\+`, `//`, `/* */` — or a character the grammar has no use for at all.
    /// The suggestion carries this language's spelling.
    UnsupportedToken,
    /// A string literal that runs to the end of the line or the end of input.
    UnterminatedString,
    /// A literal whose text cannot be a value: an unparseable float, an integer
    /// outside `i64`, a temporal that is not one, an unknown escape.
    MalformedLiteral,

    // ---- parse ----
    /// The grammar expected one thing and found another. The broadest code, and
    /// deliberately so: *expected `)`, found end of input* is already the whole
    /// diagnostic, and splitting it would name parser states rather than fixes.
    UnexpectedToken,
    /// A `,` with nothing after it — in a field list, a body, or an argument
    /// list.
    TrailingComma,
    /// A relation named as though it were a variable. Common from a Prolog
    /// prior, where the convention is the other way up.
    UppercaseRelation,
    /// `a <= b <= c`. Comparisons do not chain; the fix is the conjunction.
    ChainedComparison,
    /// A nested term where §4's flat arguments are required. The fix is a
    /// modelling change, which is why it is not folded into
    /// [`UnsupportedConstruct`](ErrorCode::UnsupportedConstruct).
    CompoundTerm,
    /// A set-builder written with something that is not one of §9's five
    /// reducers, or with no reducer at all before the `{`.
    UnknownAggregate,
    /// A shape the language does not have: disjunction in a query, a predicate
    /// with no arguments, prefix `-` on an expression, named and positional
    /// arguments mixed in one literal, a `?why` goal that is not a single atom.
    UnsupportedConstruct,

    // ---- semantic: schema ----
    /// One predicate used with two arities.
    ArityMismatch,
    /// Named arguments that do not match the relation's schema: an unknown
    /// field, a field given twice, a head that omits one, a schema that declares
    /// one twice.
    FieldMismatch,
    /// Two declarations of one relation disagree about its fields or their
    /// types.
    SchemaConflict,
    /// Named arguments used where no field names are known — the fix is to add a
    /// `declare` or an explicit import schema.
    NoSchema,
    /// A name that is already taken: a relation the program defines and a `std`
    /// module also provides, or a query named after a relation the program
    /// defines.
    NameCollision,
    /// A head variable no body literal binds (§10's range restriction).
    UnsafeRule,
    /// A variable a premise needs and nothing in the body binds.
    UnsafePremise,
    /// Premises that need each other's bindings, so no order of the body runs.
    CircularPremise,
    /// Recursion through negation or aggregation (§7's stratification).
    Unstratified,
    /// An aggregate where there is nothing to aggregate over — in a fact, or in
    /// a `?why` goal.
    AggregateMisplaced,
    /// A variable stands where a constant is required: a fact, or a `?why` goal.
    NotGround,
    /// `absent` used as a value rather than tested for (§4) — matched in a body
    /// atom's argument, or compared against with `=` / `!=`.
    AbsentMisuse,
    /// A `std` module builtin used as though it were a relation: given named
    /// arguments, negated, or handed a computed or unknown `truncate` unit.
    BuiltinMisuse,
    /// Inference reached two different types for one thing (§4).
    TypeClash,
    /// A `declare`d type contradicts the type inferred from the program.
    DeclaredTypeMismatch,
    /// An operation was given a value of the wrong type — comparison operands,
    /// arithmetic operands, a builtin's input, `min`/`max` across types, a
    /// non-numeric under `sum`/`avg`, an ambiguous temporal operand.
    TypeMismatch,
    /// An `as` cast the conversion table (§8) does not have at all.
    UnsupportedConversion,
    /// An `as` cast that exists and would **lose** the value — a timestamp to a
    /// date, a float with no exact integer. Separate from
    /// [`UnsupportedConversion`](ErrorCode::UnsupportedConversion) because the
    /// fix differs in kind: the conversion the program wants does exist, under
    /// another construct (§8's `truncate`).
    LossyConversion,
    /// Arithmetic that left the representable range, or divided by zero.
    ArithmeticError,
    /// The engine's own invariants were violated — a *malformed IR*. Not
    /// actionable by the program's author: it means a bug in the engine.
    InternalError,

    // ---- source ----
    /// An imported file is not there.
    FileNotFound,
    /// An import the engine has no reader for: an unknown extension, the
    /// reserved `table "…"` form, a `.dl` file given `as`, or a build without
    /// the `duckdb` feature.
    UnsupportedFormat,
    /// A source that exists and cannot be read: it will not open, it is not
    /// UTF-8, a fetch failed or returned nothing.
    UnreadableSource,
    /// The source's shape and the import's disagree: a field the schema names
    /// and the source lacks (or the reverse), a row with the wrong column count,
    /// an empty file where a header was required, an illegal or duplicated
    /// source field name.
    SourceSchemaMismatch,
    /// One cell that cannot become a value of its column's type.
    UnconvertibleCell,
    /// A whole column that does not fit the value model — it mixes two types, or
    /// its source type maps onto none of them.
    UnsupportedColumn,
    /// No module by that name: an unknown `std/` module, or a file that will not
    /// read.
    ModuleNotFound,
    /// A module import used as something else: a query inside a module, the
    /// reserved `std/` prefix, a URL, or an `as`-less import of a data file.
    ModuleMisuse,
}

impl ErrorCode {
    /// Every code, in declaration order. Pinned by
    /// `every_code_is_pinned_and_belongs_to_its_category`.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::UnsupportedToken,
        ErrorCode::UnterminatedString,
        ErrorCode::MalformedLiteral,
        ErrorCode::UnexpectedToken,
        ErrorCode::TrailingComma,
        ErrorCode::UppercaseRelation,
        ErrorCode::ChainedComparison,
        ErrorCode::CompoundTerm,
        ErrorCode::UnknownAggregate,
        ErrorCode::UnsupportedConstruct,
        ErrorCode::ArityMismatch,
        ErrorCode::FieldMismatch,
        ErrorCode::SchemaConflict,
        ErrorCode::NoSchema,
        ErrorCode::NameCollision,
        ErrorCode::UnsafeRule,
        ErrorCode::UnsafePremise,
        ErrorCode::CircularPremise,
        ErrorCode::Unstratified,
        ErrorCode::AggregateMisplaced,
        ErrorCode::NotGround,
        ErrorCode::AbsentMisuse,
        ErrorCode::BuiltinMisuse,
        ErrorCode::TypeClash,
        ErrorCode::DeclaredTypeMismatch,
        ErrorCode::TypeMismatch,
        ErrorCode::UnsupportedConversion,
        ErrorCode::LossyConversion,
        ErrorCode::ArithmeticError,
        ErrorCode::InternalError,
        ErrorCode::FileNotFound,
        ErrorCode::UnsupportedFormat,
        ErrorCode::UnreadableSource,
        ErrorCode::SourceSchemaMismatch,
        ErrorCode::UnconvertibleCell,
        ErrorCode::UnsupportedColumn,
        ErrorCode::ModuleNotFound,
        ErrorCode::ModuleMisuse,
    ];

    /// The stable name. Kebab-case, and the only spelling that appears in
    /// rendered output or in a machine-readable encoding of an error.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UnsupportedToken => "unsupported-token",
            ErrorCode::UnterminatedString => "unterminated-string",
            ErrorCode::MalformedLiteral => "malformed-literal",
            ErrorCode::UnexpectedToken => "unexpected-token",
            ErrorCode::TrailingComma => "trailing-comma",
            ErrorCode::UppercaseRelation => "uppercase-relation",
            ErrorCode::ChainedComparison => "chained-comparison",
            ErrorCode::CompoundTerm => "compound-term",
            ErrorCode::UnknownAggregate => "unknown-aggregate",
            ErrorCode::UnsupportedConstruct => "unsupported-construct",
            ErrorCode::ArityMismatch => "arity-mismatch",
            ErrorCode::FieldMismatch => "field-mismatch",
            ErrorCode::SchemaConflict => "schema-conflict",
            ErrorCode::NoSchema => "no-schema",
            ErrorCode::NameCollision => "name-collision",
            ErrorCode::UnsafeRule => "unsafe-rule",
            ErrorCode::UnsafePremise => "unsafe-premise",
            ErrorCode::CircularPremise => "circular-premise",
            ErrorCode::Unstratified => "unstratified",
            ErrorCode::AggregateMisplaced => "aggregate-misplaced",
            ErrorCode::NotGround => "not-ground",
            ErrorCode::AbsentMisuse => "absent-misuse",
            ErrorCode::BuiltinMisuse => "builtin-misuse",
            ErrorCode::TypeClash => "type-clash",
            ErrorCode::DeclaredTypeMismatch => "declared-type-mismatch",
            ErrorCode::TypeMismatch => "type-mismatch",
            ErrorCode::UnsupportedConversion => "unsupported-conversion",
            ErrorCode::LossyConversion => "lossy-conversion",
            ErrorCode::ArithmeticError => "arithmetic-error",
            ErrorCode::InternalError => "internal-error",
            ErrorCode::FileNotFound => "file-not-found",
            ErrorCode::UnsupportedFormat => "unsupported-format",
            ErrorCode::UnreadableSource => "unreadable-source",
            ErrorCode::SourceSchemaMismatch => "source-schema-mismatch",
            ErrorCode::UnconvertibleCell => "unconvertible-cell",
            ErrorCode::UnsupportedColumn => "unsupported-column",
            ErrorCode::ModuleNotFound => "module-not-found",
            ErrorCode::ModuleMisuse => "module-misuse",
        }
    }

    /// Which stage this code belongs to. The pairing is stated here and nowhere
    /// else, so a code cannot be raised under two categories.
    pub fn kind(self) -> ErrorKind {
        match self {
            ErrorCode::UnsupportedToken
            | ErrorCode::UnterminatedString
            | ErrorCode::MalformedLiteral => ErrorKind::Lex,

            ErrorCode::UnexpectedToken
            | ErrorCode::TrailingComma
            | ErrorCode::UppercaseRelation
            | ErrorCode::ChainedComparison
            | ErrorCode::CompoundTerm
            | ErrorCode::UnknownAggregate
            | ErrorCode::UnsupportedConstruct => ErrorKind::Parse,

            ErrorCode::ArityMismatch
            | ErrorCode::FieldMismatch
            | ErrorCode::SchemaConflict
            | ErrorCode::NoSchema
            | ErrorCode::NameCollision
            | ErrorCode::UnsafeRule
            | ErrorCode::UnsafePremise
            | ErrorCode::CircularPremise
            | ErrorCode::Unstratified
            | ErrorCode::AggregateMisplaced
            | ErrorCode::NotGround
            | ErrorCode::AbsentMisuse
            | ErrorCode::BuiltinMisuse
            | ErrorCode::TypeClash
            | ErrorCode::DeclaredTypeMismatch
            | ErrorCode::TypeMismatch
            | ErrorCode::UnsupportedConversion
            | ErrorCode::LossyConversion
            | ErrorCode::ArithmeticError
            | ErrorCode::InternalError => ErrorKind::Semantic,

            ErrorCode::FileNotFound
            | ErrorCode::UnsupportedFormat
            | ErrorCode::UnreadableSource
            | ErrorCode::SourceSchemaMismatch
            | ErrorCode::UnconvertibleCell
            | ErrorCode::UnsupportedColumn
            | ErrorCode::ModuleNotFound
            | ErrorCode::ModuleMisuse => ErrorKind::Source,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A 1-based source position, as humans and editors count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

impl Position {
    /// Resolves the position of byte offset `offset` in `source`.
    ///
    /// Columns count **characters**, not bytes, so a caret placed at the
    /// reported column lands correctly under non-ASCII text. An offset past the
    /// end clamps to the last position.
    pub fn locate(source: &str, offset: u32) -> Position {
        let offset = (offset as usize).min(source.len());
        let consumed = &source[..offset];
        let line = consumed.matches('\n').count() as u32 + 1;
        let line_start = consumed.rfind('\n').map_or(0, |index| index + 1);
        let column = source[line_start..offset].chars().count() as u32 + 1;
        Position { line, column }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A structured error produced by the engine.
///
/// Build with [`Error::lex`] / [`Error::parse`] / [`Error::semantic`] /
/// [`Error::source`], then attach what is known: [`Error::at`] for a span whose
/// position has been resolved, [`Error::suggest`] for a fix to offer. `Display`
/// composes those fields; nothing is baked into `message`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Error {
    pub kind: ErrorKind,
    /// What kind of thing is wrong, as a name a consumer can branch on.
    pub code: ErrorCode,
    /// The diagnostic sentence — no location, no suggestion, no category prefix.
    pub message: String,
    /// The byte range in the source this error is about, if the stage knew one.
    pub span: Option<Span>,
    /// `span.start` resolved against the source text, if it was in hand.
    pub position: Option<Position>,
    /// A concrete fix to offer, rendered as "(did you mean …?)"-style trailer.
    pub suggestion: Option<String>,
}

impl Error {
    /// A diagnostic under `code`, whose category the code decides.
    ///
    /// The code is a **required argument**, not a field attached afterwards.
    /// That is the span work's lesson taken literally: an optional field is how
    /// 113 sites carried no span for a month while §12 called errors structured
    /// (§17, 2026-08-25). The compiler is the only sweep that does not miss one.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Error {
        Error {
            kind: code.kind(),
            code,
            message: message.into(),
            span: None,
            position: None,
            suggestion: None,
        }
    }

    /// Attaches `span`, resolving its start against `source` for display.
    #[must_use]
    pub fn at(mut self, span: Span, source: &str) -> Error {
        self.position = Some(Position::locate(source, span.start));
        self.span = Some(span);
        self
    }

    /// Attaches `span` without resolving it.
    ///
    /// The counterpart to [`Error::at`], for the stages that run *after*
    /// parsing: lowering, type inference and the evaluator all work over the IR
    /// and never hold the program text, but the IR retains the spans of the
    /// clause, literal, query and import each of them came from. They record
    /// the span here and [`locate`](Error::locate) resolves it at the boundary
    /// that does have the source (§17, 2026-08-24).
    #[must_use]
    pub fn at_span(mut self, span: Span) -> Error {
        self.span = Some(span);
        self
    }

    /// Attaches `span` only where none is attached yet.
    ///
    /// For the evaluator, whose frames nest: a runtime error raised while
    /// enumerating one body literal unwinds through every enclosing literal on
    /// the way out, and the innermost frame is the one that knows the place. So
    /// each frame offers its span and the first offer wins.
    #[must_use]
    pub fn or_span(mut self, span: Span) -> Error {
        if self.span.is_none() {
            self.span = Some(span);
        }
        self
    }

    /// Attaches a suggested fix.
    #[must_use]
    pub fn suggest(mut self, suggestion: impl Into<String>) -> Error {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Resolves an already-attached span against `source`, so `Display` can
    /// render a line and column. A no-op when there is no span, and when the
    /// position is already known — [`Error::at`] resolved its own.
    pub fn locate(&mut self, source: &str) {
        if let (Some(span), None) = (self.span, self.position) {
            self.position = Some(Position::locate(source, span.start));
        }
    }

    /// Drops the span, and with it any claim to a location.
    ///
    /// For the case where a span is real but *unresolvable*: spans are per-file
    /// byte offsets, so once a program has spliced in a module (§13) an offset
    /// no longer identifies a place without knowing which file it counts from.
    /// Resolving it against the root text anyway would print a confidently
    /// wrong line, which is worse than printing none.
    pub fn forget_span(&mut self) {
        self.span = None;
        self.position = None;
    }
}

/// Resolves every error's span against `source` ([`Error::locate`]).
pub fn locate_all(errors: &mut [Error], source: &str) {
    for error in errors {
        error.locate(source);
    }
}

/// Drops every error's span ([`Error::forget_span`]).
pub fn forget_spans(errors: &mut [Error]) {
    for error in errors {
        error.forget_span();
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}]: {}", self.kind.label(), self.code, self.message)?;
        // Position when it was resolvable, else the raw offset — better than
        // nothing for a stage that has a span but never saw the source.
        match (self.position, self.span) {
            (Some(position), _) => write!(f, " (at {position})")?,
            (None, Some(span)) => write!(f, " (at byte {})", span.start)?,
            (None, None) => {}
        }
        if let Some(suggestion) = &self.suggestion {
            write!(f, " ({suggestion})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// A non-fatal diagnostic: the program is valid and still runs, but something
/// looks likely to be a mistake. Warnings print to **stderr** (never stdout,
/// which is reserved for the canonical fact stream) and do not change the exit
/// code. This is the first concrete piece of the §12 severity axis.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Warning {
    /// A predicate is referenced in a rule/query body but never defined (no
    /// fact, rule head, or import) — so it denotes the empty relation, which is
    /// valid Datalog but usually a typo. `suggestion` is the nearest defined
    /// predicate name, when one is close enough to be worth offering.
    UndefinedPredicate {
        name: String,
        arity: u32,
        suggestion: Option<String>,
    },
    /// A predicate that is referenced but never defined is **provided by a
    /// `std` module the program did not import** (§12/§13).
    ///
    /// This is the honest cost of gating: `year` is an ordinary undefined
    /// relation until `import "std/time".` is written, so an agent's first
    /// attempt can miss. The suggestion is what keeps that to one step, which
    /// is why it ships with the module rather than after it.
    GatedPredicate {
        name: String,
        arity: u32,
        /// The module that provides it, without the `std/` prefix.
        module: &'static str,
    },
    /// An aggregate (§9) skipped one or more `absent` inputs. `sum`/`avg`/`min`/
    /// `max` aggregate the values that exist, which is the right default but is
    /// invisible in the answer — `avg` over a half-empty column looks exactly
    /// like `avg` over a full one. §9 promises the skip is reported; until the
    /// `?why` surface (§11) lands, this warning is that report.
    ///
    /// Counted per aggregate *site* (one warning per aggregate literal that ever
    /// skipped), summed over every group it produced.
    AbsentSkippedInAggregate {
        /// The reducer, as written (`sum`, `avg`, `min`, `max`).
        op: &'static str,
        /// Where the aggregate is written.
        site: AggregateSite,
        /// Total `absent` inputs skipped across every group.
        skipped: usize,
        /// How many groups skipped at least one.
        groups: usize,
    },
    /// A conversion (§8's `as` cast) failed on data: a value existed, could not
    /// be represented in the target type, and became `absent` (§17, 2026-08-16).
    ///
    /// This is the **malformed** report, and it exists because the value model
    /// cannot carry the distinction: an absent that arrived as data (an empty
    /// CSV cell, a JSON `null`) and an absent the engine manufactured by failing
    /// a conversion are the same value (§4), so a column of malformed text reads
    /// downstream as a sparse one and nothing else would say otherwise.
    ///
    /// Counted per conversion *site*, and raised only on a run where a
    /// conversion actually failed — never statically from the presence of a
    /// cast, which is a warning that fires on correct programs
    /// (`notes/tsdl-cross-project-review.md` measured that hazard).
    ConversionFailedOnData {
        /// The target type, as written (`int`, `float`, `bool`, `symbol`).
        to: &'static str,
        /// Where the conversion is written.
        site: AggregateSite,
        /// How many values failed the conversion at this site.
        failed: usize,
    },
    /// A rule grows a predicate by **arithmetic inside a positive cycle** (§10,
    /// *Termination*), so the least model may be infinite and evaluation may not
    /// stop. The program is valid and still runs — the engine classifies and
    /// reports, it does not reject (§17, 2026-08-18).
    ///
    /// `bounded_by` is what stands between the rule and certain divergence: the
    /// relations it reads from *outside* the cycle, whose finiteness is the only
    /// thing consuming a step. Empty means nothing does, and the program cannot
    /// terminate on any input.
    ValueCreatingRecursion {
        /// The head predicate of the offending rule.
        pred: String,
        /// The concrete positive cycle, rendered `a -> b -> a`.
        cycle: String,
        /// The head variables bound to a computed value, in head-argument order.
        vars: Vec<String>,
        /// Relations read from outside the cycle, in body order.
        bounded_by: Vec<String>,
    },
}

/// Where a diagnostic's site is: inside a rule, or inside a query.
///
/// A *named* query lowers to a rule whose head is the projection (§14), so it
/// reports as a rule and only an **unnamed** query reaches [`Self::Query`] —
/// which is why the query arm has a position and not a name. The position is
/// 1-based in program order, matching the order §14 prints answer blocks in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateSite {
    /// The head predicate of the rule the site appears in, as `name/arity`.
    Rule(String),
    /// The 1-based position of the query the site appears in.
    Query(usize),
}

impl fmt::Display for AggregateSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AggregateSite::Rule(name) => write!(f, "`{name}`"),
            AggregateSite::Query(position) => write!(f, "query {position}"),
        }
    }
}

/// **What kind of thing a warning is about**, on the same terms as
/// [`ErrorCode`] (§12): stable, kebab-case, never repurposed.
///
/// A separate type because a warning is not an error and the two must not be
/// constructible from each other — but **one flat namespace**, which
/// `codes_are_unique_across_errors_and_warnings` pins: a consumer branching on
/// a slug should never have to ask which severity it came from first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum WarningCode {
    /// A predicate is referenced and never defined, so it denotes the empty
    /// relation (§10).
    UndefinedPredicate,
    /// A referenced-but-undefined predicate is one a `std` module provides and
    /// the program did not import (§13).
    GatedPredicate,
    /// An aggregate skipped `absent` inputs (§9).
    AbsentSkipped,
    /// An `as` cast failed on data, so a value that existed became `absent` —
    /// the *malformed*, not *missing*, report (§8/§12).
    ConversionFailedOnData,
    /// A recursion grows a value by arithmetic, so §6's finiteness argument does
    /// not cover it (§10).
    ValueCreatingRecursion,
}

impl WarningCode {
    /// Every warning code, in declaration order. Pinned alongside
    /// [`ErrorCode::ALL`].
    pub const ALL: &'static [WarningCode] = &[
        WarningCode::UndefinedPredicate,
        WarningCode::GatedPredicate,
        WarningCode::AbsentSkipped,
        WarningCode::ConversionFailedOnData,
        WarningCode::ValueCreatingRecursion,
    ];

    /// The stable name.
    pub fn as_str(self) -> &'static str {
        match self {
            WarningCode::UndefinedPredicate => "undefined-predicate",
            WarningCode::GatedPredicate => "gated-predicate",
            WarningCode::AbsentSkipped => "absent-skipped",
            WarningCode::ConversionFailedOnData => "conversion-failed-on-data",
            WarningCode::ValueCreatingRecursion => "value-creating-recursion",
        }
    }
}

impl fmt::Display for WarningCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Warning {
    /// What kind of thing this warning is about (§12).
    pub fn code(&self) -> WarningCode {
        match self {
            Warning::UndefinedPredicate { .. } => WarningCode::UndefinedPredicate,
            Warning::GatedPredicate { .. } => WarningCode::GatedPredicate,
            Warning::AbsentSkippedInAggregate { .. } => WarningCode::AbsentSkipped,
            Warning::ConversionFailedOnData { .. } => WarningCode::ConversionFailedOnData,
            Warning::ValueCreatingRecursion { .. } => WarningCode::ValueCreatingRecursion,
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The prefix is written once, so no arm can render a code that
        // disagrees with `Warning::code`.
        write!(f, "warning [{}]: ", self.code())?;
        match self {
            Warning::UndefinedPredicate {
                name,
                arity,
                suggestion,
            } => {
                write!(
                    f,
                    "predicate `{name}/{arity}` is referenced but never defined; \
                     it will always be empty"
                )?;
                if let Some(suggestion) = suggestion {
                    write!(f, " (did you mean `{suggestion}`?)")?;
                }
                Ok(())
            }
            Warning::GatedPredicate {
                name,
                arity,
                module,
            } => write!(
                f,
                "predicate `{name}/{arity}` is referenced but never defined; it is \
                 provided by `std/{module}` (add `import \"std/{module}\".`)"
            ),
            Warning::AbsentSkippedInAggregate {
                op,
                site,
                skipped,
                groups,
            } => write!(
                f,
                "`{op}` in {site} skipped {skipped} absent input(s) across \
                 {groups} group(s); the result covers only the values that exist"
            ),
            Warning::ConversionFailedOnData { to, site, failed } => write!(
                f,
                "`as {to}` in {site} produced absent for {failed} value(s) that \
                 could not be represented; they are malformed, not missing"
            ),
            Warning::ValueCreatingRecursion {
                pred,
                cycle,
                vars,
                bounded_by,
            } => {
                let vars = vars
                    .iter()
                    .map(|var| format!("`{var}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "value-creating recursion: `{pred}` grows by arithmetic ({vars}) \
                     inside the positive cycle `{cycle}`"
                )?;
                if bounded_by.is_empty() {
                    write!(
                        f,
                        "; nothing outside the cycle bounds it, so this program does not terminate"
                    )
                } else {
                    let relations = bounded_by
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    write!(
                        f,
                        "; it terminates only while {relations} has no cycle reachable through \
                         this rule"
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_one_based_lines_and_columns() {
        let src = "p(1).\nq(2).\n\nr(3).";
        assert_eq!(Position::locate(src, 0), Position { line: 1, column: 1 });
        assert_eq!(Position::locate(src, 3), Position { line: 1, column: 4 });
        // First byte of line 2.
        assert_eq!(Position::locate(src, 6), Position { line: 2, column: 1 });
        // The blank line 3, then line 4.
        assert_eq!(Position::locate(src, 12), Position { line: 3, column: 1 });
        assert_eq!(Position::locate(src, 13), Position { line: 4, column: 1 });
    }

    /// Columns count characters, not bytes, so a caret placed at the reported
    /// column lands under the right glyph in non-ASCII source.
    #[test]
    fn columns_count_characters_not_bytes() {
        let src = "p(\"héllo\", X).";
        let byte_of_x = src.find('X').expect("X is present") as u32;
        // 'é' is two bytes but one column.
        assert_eq!(byte_of_x, 12);
        assert_eq!(
            Position::locate(src, byte_of_x),
            Position {
                line: 1,
                column: 12
            }
        );
    }

    #[test]
    fn an_offset_past_the_end_clamps() {
        let src = "p(1).";
        assert_eq!(
            Position::locate(src, 9_999),
            Position { line: 1, column: 6 }
        );
    }

    /// `Display` composes the fields; nothing is baked into `message`.
    #[test]
    fn display_is_built_from_the_fields() {
        let src = "p(1).\nq(2).";
        let span = Span { start: 6, end: 7 };
        let plain = Error::new(ErrorCode::UnsafeRule, "something is wrong");
        assert_eq!(
            plain.to_string(),
            "semantic error [unsafe-rule]: something is wrong"
        );

        let located = Error::new(ErrorCode::UnexpectedToken, "expected `.`").at(span, src);
        assert_eq!(located.position, Some(Position { line: 2, column: 1 }));
        assert_eq!(
            located.to_string(),
            "syntax error [unexpected-token]: expected `.` (at 2:1)"
        );

        let full = Error::new(ErrorCode::UnsupportedToken, "`=<` is not an operator")
            .at(span, src)
            .suggest("did you mean `<=`?");
        assert_eq!(
            full.to_string(),
            "lexical error [unsupported-token]: `=<` is not an operator (at 2:1) \
             (did you mean `<=`?)"
        );
    }

    /// A stage that has a span but never saw the source still reports something
    /// better than nothing.
    #[test]
    fn a_span_without_a_source_falls_back_to_the_byte_offset() {
        let mut error = Error::new(ErrorCode::UnsafeRule, "unsafe rule");
        error.span = Some(Span { start: 42, end: 45 });
        assert_eq!(
            error.to_string(),
            "semantic error [unsafe-rule]: unsafe rule (at byte 42)"
        );
    }

    /// **The vocabulary is pinned.** Every code, sorted, against a literal list.
    ///
    /// The contract §12 states is that codes are stable and additive: adding one
    /// is compatible, renaming or removing one is a breaking change. The
    /// compiler already forces a new variant through `as_str` and `kind`; what
    /// it cannot see is a *rename*, which to a consumer is a code silently
    /// disappearing. This test is where that shows up, and where a reviewer sees
    /// an addition as a diff rather than as nothing.
    #[test]
    fn every_code_is_pinned_and_belongs_to_its_category() {
        let mut slugs: Vec<&str> = ErrorCode::ALL.iter().map(|code| code.as_str()).collect();
        slugs.sort_unstable();
        assert_eq!(
            slugs,
            [
                "absent-misuse",
                "aggregate-misplaced",
                "arithmetic-error",
                "arity-mismatch",
                "builtin-misuse",
                "chained-comparison",
                "circular-premise",
                "compound-term",
                "declared-type-mismatch",
                "field-mismatch",
                "file-not-found",
                "internal-error",
                "lossy-conversion",
                "malformed-literal",
                "module-misuse",
                "module-not-found",
                "name-collision",
                "no-schema",
                "not-ground",
                "schema-conflict",
                "source-schema-mismatch",
                "trailing-comma",
                "type-clash",
                "type-mismatch",
                "unconvertible-cell",
                "unexpected-token",
                "unknown-aggregate",
                "unreadable-source",
                "unsafe-premise",
                "unsafe-rule",
                "unstratified",
                "unsupported-column",
                "unsupported-construct",
                "unsupported-conversion",
                "unsupported-format",
                "unsupported-token",
                "unterminated-string",
                "uppercase-relation",
            ]
        );

        // A code belongs to exactly one category, and an `Error` built from it
        // reports that category — the two axes cannot disagree.
        for &code in ErrorCode::ALL {
            assert_eq!(Error::new(code, "a message").kind, code.kind());
        }
    }

    /// One flat namespace across both severities.
    ///
    /// The types are separate so a warning cannot be constructed as an error,
    /// but a consumer branching on a slug should never have to ask which
    /// severity it came from first — so no slug may appear in both sets, or
    /// twice in either.
    #[test]
    fn codes_are_unique_across_errors_and_warnings() {
        let mut slugs: Vec<&str> = ErrorCode::ALL
            .iter()
            .map(|code| code.as_str())
            .chain(WarningCode::ALL.iter().map(|code| code.as_str()))
            .collect();
        let total = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), total, "a slug is used twice");
        assert_eq!(total, 43, "38 error codes and 5 warning codes");
    }

    /// Every warning renders its own code, and only its own.
    #[test]
    fn a_warning_renders_the_code_it_reports() {
        let warning = Warning::UndefinedPredicate {
            name: "p".to_string(),
            arity: 1,
            suggestion: None,
        };
        assert_eq!(warning.code(), WarningCode::UndefinedPredicate);
        assert!(
            warning
                .to_string()
                .starts_with("warning [undefined-predicate]: "),
            "rendered as {warning}"
        );
    }
}
