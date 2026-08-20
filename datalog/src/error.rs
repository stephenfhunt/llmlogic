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
//! Still to come (§12, tracked in `ROADMAP.md`): a stable machine-readable
//! **code** vocabulary, and spans on the semantic/source errors — lowering
//! reports many of its errors from places where the responsible span is not
//! currently threaded, and picking the right one per error is a design pass, not
//! a mechanical change.

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
    fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        Error {
            kind,
            message: message.into(),
            span: None,
            position: None,
            suggestion: None,
        }
    }

    /// A lexical error.
    pub fn lex(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::Lex, message)
    }

    /// A syntax error.
    pub fn parse(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::Parse, message)
    }

    /// A semantic/safety error.
    pub fn semantic(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::Semantic, message)
    }

    /// An external-source error (§13).
    pub fn source(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::Source, message)
    }

    /// Attaches `span`, resolving its start against `source` for display.
    #[must_use]
    pub fn at(mut self, span: Span, source: &str) -> Error {
        self.position = Some(Position::locate(source, span.start));
        self.span = Some(span);
        self
    }

    /// Attaches a suggested fix.
    #[must_use]
    pub fn suggest(mut self, suggestion: impl Into<String>) -> Error {
        self.suggestion = Some(suggestion.into());
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.label(), self.message)?;
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

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::UndefinedPredicate {
                name,
                arity,
                suggestion,
            } => {
                write!(
                    f,
                    "warning: predicate `{name}/{arity}` is referenced but never defined; \
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
                "warning: predicate `{name}/{arity}` is referenced but never defined; it is \
                 provided by `std/{module}` (add `import \"std/{module}\".`)"
            ),
            Warning::AbsentSkippedInAggregate {
                op,
                site,
                skipped,
                groups,
            } => write!(
                f,
                "warning: `{op}` in {site} skipped {skipped} absent input(s) across \
                 {groups} group(s); the result covers only the values that exist"
            ),
            Warning::ConversionFailedOnData { to, site, failed } => write!(
                f,
                "warning: `as {to}` in {site} produced absent for {failed} value(s) that \
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
                    "warning: value-creating recursion: `{pred}` grows by arithmetic ({vars}) \
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
        let plain = Error::semantic("something is wrong");
        assert_eq!(plain.to_string(), "semantic error: something is wrong");

        let located = Error::parse("expected `.`").at(span, src);
        assert_eq!(located.position, Some(Position { line: 2, column: 1 }));
        assert_eq!(located.to_string(), "syntax error: expected `.` (at 2:1)");

        let full = Error::lex("`=<` is not an operator")
            .at(span, src)
            .suggest("did you mean `<=`?");
        assert_eq!(
            full.to_string(),
            "lexical error: `=<` is not an operator (at 2:1) (did you mean `<=`?)"
        );
    }

    /// A stage that has a span but never saw the source still reports something
    /// better than nothing.
    #[test]
    fn a_span_without_a_source_falls_back_to_the_byte_offset() {
        let mut error = Error::semantic("unsafe rule");
        error.span = Some(Span { start: 42, end: 45 });
        assert_eq!(
            error.to_string(),
            "semantic error: unsafe rule (at byte 42)"
        );
    }
}
