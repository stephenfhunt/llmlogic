//! Hand-rolled lexer: source text → a flat token stream with spans (`spec.md`
//! §3).
//!
//! Zero dependencies by design (spec §17, Phase D): the LLM-friendly-errors
//! pillar wants full control over spans and *targeted* messages for the
//! near-miss spellings models reach for out of a Prolog prior (`=<` for `<=`,
//! `\=` for `!=`, `\+`/`!` for `not`, `//` comments). Rather than reject those
//! with a bare "unexpected character", the lexer recognizes each, records an
//! actionable [`Error::Lex`], and **substitutes the intended token** so parsing
//! continues cleanly instead of cascading.
//!
//! Lexing never fails hard: [`lex`] returns whatever tokens it could form
//! *and* every error it hit (`testing.md` D4 — never panic on arbitrary
//! bytes). Unknown characters and unterminated strings are recorded and
//! skipped; the parser recovers at the next `.`.
//!
//! Numbers are lexed **unsigned** — a leading `-` is always the [`Minus`]
//! operator, folded onto a numeric literal by the parser in operand position
//! (spec §17, Phase D, decision 3). The five type names (`int`, `float`,
//! `string`, `symbol`, `bool`) are *contextual*: they lex as [`Ident`] and are
//! recognized only in a declaration/import field-type position by the parser.
//!
//! [`Minus`]: TokenKind::Minus
//! [`Ident`]: TokenKind::Ident

use crate::ast::Span;
use crate::error::Error;

/// A lexical token: its kind plus the source span it covers.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// The lexical categories of §3.
///
/// Keywords (`import as declare not true false absent is`) are distinguished from
/// [`Ident`](TokenKind::Ident) here so the parser never string-matches; the
/// five type names are *not* keywords (they stay `Ident`, recognized in
/// context).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A lowercase-initial identifier: relation name, symbol spelling, field
    /// name, or a contextual type name.
    Ident(String),
    /// An uppercase- or `_`-initial variable. A lone `_` keeps its spelling
    /// here; the parser maps it to [`crate::ast::TermKind::Wildcard`].
    Variable(String),
    /// A non-negative integer literal (sign is a separate [`Minus`](Self::Minus)).
    Int(i64),
    /// A non-negative float literal.
    Float(f64),
    /// A string literal, with escapes already resolved.
    Str(String),
    /// An `@`-sigilled temporal literal (§3), already read into its value —
    /// the components are validated here so `@2026-02-30` names the day rather
    /// than reaching the parser as three integers.
    Temporal(crate::temporal::Temporal),

    // Keywords (§3 reserved words).
    Import,
    As,
    Declare,
    Not,
    True,
    False,
    /// The missing-data value literal (§4). Reserved: not an identifier.
    Absent,
    /// The presence-test operator head, `is` in `is [not] absent` (§4/§8).
    Is,

    // Punctuation / operators (§3).
    /// `:-`
    ColonDash,
    /// `?-`
    QuestionDash,
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `:`
    Colon,
    /// `;`
    Semi,
    /// `=`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `{` — opens an aggregate (§9).
    LBrace,
    /// `}` — closes an aggregate (§9).
    RBrace,
    /// `|` — the set-builder separator in an aggregate `op { expr | goal }` (§9).
    Pipe,

    /// End of input — always the final token, so the parser can peek without
    /// bounds checks.
    Eof,
}

impl TokenKind {
    /// A short human name for this token, for parser error messages.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Ident(s) => format!("identifier `{s}`"),
            TokenKind::Variable(s) => format!("variable `{s}`"),
            TokenKind::Int(n) => format!("integer `{n}`"),
            TokenKind::Float(f) => format!("float `{f}`"),
            TokenKind::Str(s) => format!("string {s:?}"),
            TokenKind::Temporal(value) => format!("temporal literal `@{value}`"),
            TokenKind::Import => "`import`".to_string(),
            TokenKind::As => "`as`".to_string(),
            TokenKind::Declare => "`declare`".to_string(),
            TokenKind::Not => "`not`".to_string(),
            TokenKind::True => "`true`".to_string(),
            TokenKind::False => "`false`".to_string(),
            TokenKind::Absent => "`absent`".to_string(),
            TokenKind::Is => "`is`".to_string(),
            TokenKind::ColonDash => "`:-`".to_string(),
            TokenKind::QuestionDash => "`?-`".to_string(),
            TokenKind::Dot => "`.`".to_string(),
            TokenKind::Comma => "`,`".to_string(),
            TokenKind::LParen => "`(`".to_string(),
            TokenKind::RParen => "`)`".to_string(),
            TokenKind::Colon => "`:`".to_string(),
            TokenKind::Semi => "`;`".to_string(),
            TokenKind::Eq => "`=`".to_string(),
            TokenKind::Ne => "`!=`".to_string(),
            TokenKind::Lt => "`<`".to_string(),
            TokenKind::Le => "`<=`".to_string(),
            TokenKind::Gt => "`>`".to_string(),
            TokenKind::Ge => "`>=`".to_string(),
            TokenKind::Plus => "`+`".to_string(),
            TokenKind::Minus => "`-`".to_string(),
            TokenKind::Star => "`*`".to_string(),
            TokenKind::Slash => "`/`".to_string(),
            TokenKind::LBrace => "`{`".to_string(),
            TokenKind::RBrace => "`}`".to_string(),
            TokenKind::Pipe => "`|`".to_string(),
            TokenKind::Eof => "end of input".to_string(),
        }
    }
}

/// The tokens and any lexical errors found. Errors are recorded, never fatal:
/// the token stream is always usable and always ends in [`TokenKind::Eof`].
#[derive(Debug, Clone, PartialEq)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub errors: Vec<Error>,
}

/// Tokenizes `src`, recording every lexical error rather than stopping at the
/// first (§12 pillar; D4 never-panic).
pub fn lex(src: &str) -> LexResult {
    Lexer::new(src).run()
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    errors: Vec<Error>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Lexer<'a> {
        Lexer {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn run(mut self) -> LexResult {
        while self.pos < self.bytes.len() {
            self.scan_one();
        }
        let end = self.bytes.len() as u32;
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span { start: end, end },
        });
        LexResult {
            tokens: self.tokens,
            errors: self.errors,
        }
    }

    fn peek(&self) -> u8 {
        self.bytes[self.pos]
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: Span {
                start: start as u32,
                end: self.pos as u32,
            },
        });
    }

    /// Records a lexical error spanning from `start` to the current position,
    /// with its source position resolved (§12) — the location is structured
    /// data on the error, never text baked into the message.
    fn error(&mut self, start: usize, message: impl Into<String>) {
        let span = Span {
            start: start as u32,
            end: self.pos.max(start) as u32,
        };
        self.errors.push(Error::lex(message).at(span, self.src));
    }

    /// [`Self::error`] plus a structured suggested fix (§12) — the near-miss
    /// hints, which used to be sentence fragments inside the message.
    fn error_suggesting(
        &mut self,
        start: usize,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) {
        let span = Span {
            start: start as u32,
            end: self.pos.max(start) as u32,
        };
        self.errors
            .push(Error::lex(message).at(span, self.src).suggest(suggestion));
    }

    fn scan_one(&mut self) {
        let start = self.pos;
        let c = self.peek();
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => {
                self.pos += 1;
            }
            // `%` and `#` both begin a line comment (§3, plus the `#` alias —
            // spec §17, Phase D). `//` is *not* a comment: reserved against a
            // future floor-division operator, so it gets a targeted hint.
            b'%' | b'#' => self.skip_line(),
            b'"' | b'\'' => self.scan_string(c),
            // `@` begins nothing else in the grammar, so the lexer commits on
            // the sigil and reads the whole literal as one token (§5).
            b'@' => self.scan_temporal(),
            b'0'..=b'9' => self.scan_number(),
            b'a'..=b'z' => self.scan_ident_or_keyword(),
            b'A'..=b'Z' | b'_' => self.scan_variable(),
            b'(' => self.punct(TokenKind::LParen, 1),
            b')' => self.punct(TokenKind::RParen, 1),
            b',' => self.punct(TokenKind::Comma, 1),
            b';' => self.punct(TokenKind::Semi, 1),
            b'.' => self.punct(TokenKind::Dot, 1),
            b'+' => self.punct(TokenKind::Plus, 1),
            b'-' => self.punct(TokenKind::Minus, 1),
            b'*' => self.punct(TokenKind::Star, 1),
            b'{' => self.punct(TokenKind::LBrace, 1),
            b'}' => self.punct(TokenKind::RBrace, 1),
            b'|' => self.punct(TokenKind::Pipe, 1),
            b':' => {
                if self.peek_at(1) == Some(b'-') {
                    self.punct(TokenKind::ColonDash, 2);
                } else {
                    self.punct(TokenKind::Colon, 1);
                }
            }
            b'?' => {
                if self.peek_at(1) == Some(b'-') {
                    self.punct(TokenKind::QuestionDash, 2);
                } else {
                    self.pos += 1;
                    self.error(start, "stray `?`; a query is written `?- <body>.`");
                }
            }
            b'=' => {
                // `=<` is Prolog's `<=`; recognize it, hint, and substitute.
                if self.peek_at(1) == Some(b'<') {
                    self.pos += 2;
                    self.error_suggesting(start, "`=<` is not an operator", "did you mean `<=`?");
                    self.push(TokenKind::Le, start);
                } else {
                    self.punct(TokenKind::Eq, 1);
                }
            }
            b'!' => {
                if self.peek_at(1) == Some(b'=') {
                    self.punct(TokenKind::Ne, 2);
                } else {
                    // Prolog cut / negation; here negation is the `not` keyword.
                    self.pos += 1;
                    self.error(start, "`!` is not an operator; negation is written `not`");
                    self.push(TokenKind::Not, start);
                }
            }
            b'<' => {
                if self.peek_at(1) == Some(b'=') {
                    self.punct(TokenKind::Le, 2);
                } else {
                    self.punct(TokenKind::Lt, 1);
                }
            }
            b'>' => {
                if self.peek_at(1) == Some(b'=') {
                    self.punct(TokenKind::Ge, 2);
                } else {
                    self.punct(TokenKind::Gt, 1);
                }
            }
            b'/' => match self.peek_at(1) {
                Some(b'/') => {
                    self.error(
                        start,
                        "`//` is not a comment; comments start with `%` or `#`",
                    );
                    self.skip_line();
                }
                Some(b'*') => {
                    self.error(
                        start,
                        "`/* */` block comments are not supported; use `%` or `#` to end of line",
                    );
                    self.skip_block_comment();
                }
                _ => self.punct(TokenKind::Slash, 1),
            },
            b'\\' => {
                // `\=` (Prolog inequality) and `\+` (Prolog negation).
                match self.peek_at(1) {
                    Some(b'=') => {
                        self.pos += 2;
                        self.error_suggesting(
                            start,
                            "`\\=` is not an operator",
                            "did you mean `!=`?",
                        );
                        self.push(TokenKind::Ne, start);
                    }
                    Some(b'+') => {
                        self.pos += 2;
                        self.error(start, "`\\+` is not an operator; negation is written `not`");
                        self.push(TokenKind::Not, start);
                    }
                    _ => {
                        self.pos += 1;
                        self.error(start, "unexpected `\\`");
                    }
                }
            }
            _ => self.scan_unknown(start),
        }
    }

    fn punct(&mut self, kind: TokenKind, len: usize) {
        let start = self.pos;
        self.pos += len;
        self.push(kind, start);
    }

    fn skip_line(&mut self) {
        while self.pos < self.bytes.len() && self.peek() != b'\n' {
            self.pos += 1;
        }
    }

    fn skip_block_comment(&mut self) {
        // Best-effort recovery for the rejected `/* */` form: skip to `*/` if
        // present, else to end of input.
        self.pos += 2;
        while self.pos + 1 < self.bytes.len() {
            if self.peek() == b'*' && self.peek_at(1) == Some(b'/') {
                self.pos += 2;
                return;
            }
            self.pos += 1;
        }
        self.pos = self.bytes.len();
    }

    fn scan_string(&mut self, quote: u8) {
        let start = self.pos;
        self.pos += 1; // opening quote
        let mut value = String::new();
        loop {
            if self.pos >= self.bytes.len() {
                self.error(start, "unterminated string literal");
                break;
            }
            let c = self.peek();
            if c == quote {
                self.pos += 1;
                self.push(TokenKind::Str(value), start);
                return;
            }
            if c == b'\n' {
                self.error(start, "unterminated string literal");
                break;
            }
            if c == b'\\' {
                // The escaped item is the *character* after the backslash,
                // which may be multi-byte — advance by its full width so the
                // cursor stays on a char boundary (D4).
                let esc_start = self.pos + 1;
                let Some(esc) = self.src[esc_start..].chars().next() else {
                    self.error(start, "unterminated string literal");
                    self.pos = self.bytes.len();
                    break;
                };
                match esc {
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    other => {
                        self.error(
                            esc_start,
                            format!(
                                "unknown escape `\\{other}`; valid escapes are \\\\ \\\" \\' \\n \\t"
                            ),
                        );
                    }
                }
                self.pos = esc_start + esc.len_utf8();
                continue;
            }
            // Copy one full UTF-8 char so string *contents* may be any Unicode.
            let ch = self.src[self.pos..].chars().next().expect("in-bounds char");
            value.push(ch);
            self.pos += ch.len_utf8();
        }
    }

    /// Reads an `@`-sigilled temporal literal (§3).
    ///
    /// The run is delimited *structurally*, not greedily: a `-` is taken only
    /// where a date has one (offsets 4 and 7) or as a duration's leading sign,
    /// so `@2026-08-20-@2026-08-19` is two literals and a subtraction rather
    /// than one unreadable token. A `.` is taken only when a digit follows,
    /// the rule `p(1).` already needs.
    fn scan_temporal(&mut self) {
        let start = self.pos;
        self.pos += 1; // `@`
        let body_start = self.pos;
        if self.peek_at(0) == Some(b'-') {
            self.pos += 1;
        }
        let digits_start = self.pos;
        while let Some(byte) = self.peek_at(0) {
            let offset = self.pos - digits_start;
            let take = match byte {
                b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b':' | b'+' => true,
                b'-' => offset == 4 || offset == 7,
                b'.' => self.peek_at(1).is_some_and(|b| b.is_ascii_digit()),
                _ => false,
            };
            if !take {
                break;
            }
            self.pos += 1;
        }
        let text = &self.src[body_start..self.pos];
        if text.is_empty() {
            self.error_suggesting(
                start,
                "`@` begins a temporal literal but none follows",
                "a date `@2026-08-19`, a timestamp `@2026-08-19T10:30:00`, or a \
                 duration `@1d12h`",
            );
            return;
        }
        match crate::temporal::parse_temporal(text) {
            Ok(value) => self.push(TokenKind::Temporal(value), start),
            Err(error) => {
                let message = format!("`@{text}` is not a temporal literal: {}", error.message());
                match error.suggestion() {
                    Some(suggestion) => self.error_suggesting(start, message, suggestion),
                    None => self.error(start, message),
                }
            }
        }
    }

    fn scan_number(&mut self) {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.peek().is_ascii_digit() {
            self.pos += 1;
        }
        let mut is_float = false;
        // A `.` is part of the number only if a digit follows — otherwise it is
        // the statement terminator (`p(1).`).
        if self.peek_at(0) == Some(b'.') && self.peek_at(1).is_some_and(|b| b.is_ascii_digit()) {
            is_float = true;
            self.pos += 1;
            while self.pos < self.bytes.len() && self.peek().is_ascii_digit() {
                self.pos += 1;
            }
        }
        // Exponent: `e`/`E` with an optional sign and at least one digit.
        if matches!(self.peek_at(0), Some(b'e' | b'E')) {
            let mut probe = 1;
            if matches!(self.peek_at(probe), Some(b'+' | b'-')) {
                probe += 1;
            }
            if self.peek_at(probe).is_some_and(|b| b.is_ascii_digit()) {
                is_float = true;
                self.pos += probe;
                while self.pos < self.bytes.len() && self.peek().is_ascii_digit() {
                    self.pos += 1;
                }
            }
        }
        let text = &self.src[start..self.pos];
        if is_float {
            match text.parse::<f64>() {
                Ok(f) if f.is_finite() => self.push(TokenKind::Float(f), start),
                _ => {
                    self.error(start, format!("invalid float literal `{text}`"));
                }
            }
        } else {
            match text.parse::<i64>() {
                Ok(n) => self.push(TokenKind::Int(n), start),
                Err(_) => {
                    self.error(
                        start,
                        format!("integer literal `{text}` is out of range for i64"),
                    );
                }
            }
        }
    }

    fn scan_ident_or_keyword(&mut self) {
        let start = self.pos;
        self.consume_ident_chars();
        let text = &self.src[start..self.pos];
        let kind = match text {
            "import" => TokenKind::Import,
            "as" => TokenKind::As,
            "declare" => TokenKind::Declare,
            "not" => TokenKind::Not,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "absent" => TokenKind::Absent,
            "is" => TokenKind::Is,
            _ => TokenKind::Ident(text.to_string()),
        };
        self.push(kind, start);
    }

    fn scan_variable(&mut self) {
        let start = self.pos;
        self.consume_ident_chars();
        let text = self.src[start..self.pos].to_string();
        self.push(TokenKind::Variable(text), start);
    }

    fn consume_ident_chars(&mut self) {
        while self.pos < self.bytes.len() {
            let c = self.peek();
            if c == b'_' || c.is_ascii_alphanumeric() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn scan_unknown(&mut self, start: usize) {
        // Smart/curly quotes are a common paste artifact — give them a dedicated
        // hint before the generic path.
        let ch = self.src[start..].chars().next().unwrap_or('\u{FFFD}');
        let len = ch.len_utf8();
        self.pos += len;
        match ch {
            '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}' => {
                self.error(start, "curly quote; use a straight quote `\"` or `'`");
            }
            _ => {
                self.error(start, format!("unexpected character `{ch}`"));
            }
        }
    }
}

/// How text reads under the language's literal grammar (§13/§17, 2026-07-23).
///
/// [`Str`](CellClass::Str) is the fall-through: text that is not exactly one
/// literal is a string.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CellClass {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str,
}

/// Classifies text by the language's literal grammar: [`lex`] must read the
/// whole of it as exactly one (optionally signed) literal. Anything else —
/// empty text, multiple tokens, lexical errors — is a string.
///
/// This is the language's **only** text-to-value classifier, and both of its
/// callers depend on that. §13 types an untyped CSV cell with it, so an import
/// means precisely the facts you would get by writing its cells as in-program
/// literals; §8's `as` cast converts *from* `string` with it, so `"30" as int`
/// is `30` exactly when writing `30` would be that literal. A second classifier
/// would let the two answers drift.
pub(crate) fn classify_cell(text: &str) -> CellClass {
    let lexed = lex(text);
    if !lexed.errors.is_empty() {
        return CellClass::Str;
    }
    let kinds: Vec<&TokenKind> = lexed.tokens.iter().map(|t| &t.kind).collect();
    match kinds.as_slice() {
        [TokenKind::Int(n), TokenKind::Eof] => CellClass::Int(*n),
        [TokenKind::Minus, TokenKind::Int(n), TokenKind::Eof] => CellClass::Int(-*n),
        [TokenKind::Float(f), TokenKind::Eof] => CellClass::Float(*f),
        [TokenKind::Minus, TokenKind::Float(f), TokenKind::Eof] => CellClass::Float(-*f),
        [TokenKind::True, TokenKind::Eof] => CellClass::Bool(true),
        [TokenKind::False, TokenKind::Eof] => CellClass::Bool(false),
        _ => CellClass::Str,
    }
}

/// The symbol reading of text: a single bare identifier token. §13 uses it only
/// under an explicit `symbol` schema type (inference never produces symbol);
/// §8's cast uses it for `string as symbol`.
pub(crate) fn classify_symbol(text: &str) -> Option<String> {
    let lexed = lex(text);
    if !lexed.errors.is_empty() {
        return None;
    }
    match lexed.tokens.as_slice() {
        [ident, eof] if matches!(eof.kind, TokenKind::Eof) => match &ident.kind {
            TokenKind::Ident(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src)
            .tokens
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| *k != TokenKind::Eof)
            .collect()
    }

    fn temporal(text: &str) -> TokenKind {
        TokenKind::Temporal(crate::temporal::parse_temporal(text).expect("a temporal literal"))
    }

    #[test]
    fn lexes_the_three_temporal_literals() {
        assert_eq!(
            kinds("p(@2026-08-19, @2026-08-19T10:30:00.5, @1d12h)."),
            vec![
                TokenKind::Ident("p".into()),
                TokenKind::LParen,
                temporal("2026-08-19"),
                TokenKind::Comma,
                temporal("2026-08-19T10:30:00.5"),
                TokenKind::Comma,
                temporal("1d12h"),
                TokenKind::RParen,
                TokenKind::Dot,
            ]
        );
    }

    #[test]
    fn a_temporal_literal_does_not_swallow_the_terminator_or_a_minus() {
        // The `.` after a date is the statement terminator, not a fraction —
        // the same digit-must-follow rule `p(1).` needs.
        assert_eq!(
            kinds("p(@1d)."),
            vec![
                TokenKind::Ident("p".into()),
                TokenKind::LParen,
                temporal("1d"),
                TokenKind::RParen,
                TokenKind::Dot,
            ]
        );
        // A `-` is taken only where a date has one, so a difference written
        // without spaces is two literals and a subtraction.
        assert_eq!(
            kinds("@2026-08-20-@2026-08-19"),
            vec![
                temporal("2026-08-20"),
                TokenKind::Minus,
                temporal("2026-08-19"),
            ]
        );
        // And a negative duration keeps its sign, because it is at offset 0.
        assert_eq!(kinds("@-1d12h"), vec![temporal("-1d12h")]);
    }

    #[test]
    fn a_malformed_temporal_literal_names_the_component() {
        let errors = lex("p(@2026-02-30).").errors;
        assert_eq!(errors.len(), 1, "{errors:?}");
        let message = errors[0].to_string();
        assert!(message.contains("30 is not a valid day"), "{message}");
        // A calendar unit is refused with the reason, and the fix.
        let errors = lex("p(@P1M).").errors;
        let message = errors[0].to_string();
        assert!(message.contains("no fixed length"), "{message}");
        assert!(message.contains("truncate"), "{message}");
        // A zone offset says the value model is civil rather than "bad shape".
        let errors = lex("p(@2026-08-19T10:30:00Z).").errors;
        let message = errors[0].to_string();
        assert!(message.contains("civil"), "{message}");
        // A bare sigil is its own message.
        let errors = lex("p(@).").errors;
        assert!(errors[0].to_string().contains("none follows"), "{errors:?}");
    }

    #[test]
    fn an_unsigilled_date_is_still_arithmetic() {
        // The sigil is load-bearing: accepting the bare form would silently
        // change what every existing `2026-08-19` means (§17).
        assert_eq!(
            kinds("2026-08-19"),
            vec![
                TokenKind::Int(2026),
                TokenKind::Minus,
                TokenKind::Int(8),
                TokenKind::Minus,
                TokenKind::Int(19),
            ]
        );
    }

    #[test]
    fn lexes_a_fact() {
        assert_eq!(
            kinds("parent(\"alice\", \"bob\")."),
            vec![
                TokenKind::Ident("parent".into()),
                TokenKind::LParen,
                TokenKind::Str("alice".into()),
                TokenKind::Comma,
                TokenKind::Str("bob".into()),
                TokenKind::RParen,
                TokenKind::Dot,
            ]
        );
    }

    #[test]
    fn lexes_a_rule_with_variables_and_query() {
        assert_eq!(
            kinds("ancestor(X, Y) :- parent(X, Y).\n?- ancestor(\"a\", Who)."),
            vec![
                TokenKind::Ident("ancestor".into()),
                TokenKind::LParen,
                TokenKind::Variable("X".into()),
                TokenKind::Comma,
                TokenKind::Variable("Y".into()),
                TokenKind::RParen,
                TokenKind::ColonDash,
                TokenKind::Ident("parent".into()),
                TokenKind::LParen,
                TokenKind::Variable("X".into()),
                TokenKind::Comma,
                TokenKind::Variable("Y".into()),
                TokenKind::RParen,
                TokenKind::Dot,
                TokenKind::QuestionDash,
                TokenKind::Ident("ancestor".into()),
                TokenKind::LParen,
                TokenKind::Str("a".into()),
                TokenKind::Comma,
                TokenKind::Variable("Who".into()),
                TokenKind::RParen,
                TokenKind::Dot,
            ]
        );
    }

    #[test]
    fn keywords_and_wildcard() {
        assert_eq!(
            kinds("import declare not true false absent is _ _X"),
            vec![
                TokenKind::Import,
                TokenKind::Declare,
                TokenKind::Not,
                TokenKind::True,
                TokenKind::False,
                TokenKind::Absent,
                TokenKind::Is,
                TokenKind::Variable("_".into()),
                TokenKind::Variable("_X".into()),
            ]
        );
    }

    #[test]
    fn int_vs_float_and_dot_terminator() {
        // `p(1).` — the trailing `.` is the terminator, not part of the number.
        assert_eq!(
            kinds("p(1)."),
            vec![
                TokenKind::Ident("p".into()),
                TokenKind::LParen,
                TokenKind::Int(1),
                TokenKind::RParen,
                TokenKind::Dot,
            ]
        );
        assert_eq!(
            kinds("3.5 6.02e23 1e10 42"),
            vec![
                TokenKind::Float(3.5),
                TokenKind::Float(6.02e23),
                TokenKind::Float(1e10),
                TokenKind::Int(42),
            ]
        );
    }

    #[test]
    fn minus_is_always_an_operator_token() {
        // Signed literals are folded by the parser, not the lexer (decision 3).
        assert_eq!(kinds("-7"), vec![TokenKind::Minus, TokenKind::Int(7)]);
    }

    #[test]
    fn both_comment_markers_and_string_escapes() {
        assert_eq!(
            kinds("% a comment\n# another\nx"),
            vec![TokenKind::Ident("x".into())]
        );
        assert_eq!(
            kinds(r#""a\tb\nc\\d\"e""#),
            vec![TokenKind::Str("a\tb\nc\\d\"e".into())]
        );
    }

    #[test]
    fn single_quoted_strings_are_strings() {
        assert_eq!(kinds("'alice'"), vec![TokenKind::Str("alice".into())]);
    }

    #[test]
    fn near_miss_operators_are_substituted_with_hints() {
        for (src, expected, hint) in [
            ("=<", TokenKind::Le, "did you mean `<=`?"),
            ("\\=", TokenKind::Ne, "did you mean `!=`?"),
            ("\\+", TokenKind::Not, "negation is written `not`"),
            ("!", TokenKind::Not, "negation is written `not`"),
        ] {
            let result = lex(src);
            let kinds: Vec<_> = result
                .tokens
                .iter()
                .map(|t| &t.kind)
                .filter(|k| **k != TokenKind::Eof)
                .collect();
            assert_eq!(kinds, vec![&expected], "for {src:?}");
            assert!(
                result.errors.iter().any(|e| e.to_string().contains(hint)),
                "expected hint {hint:?} for {src:?}, got {:?}",
                result.errors
            );
        }
    }

    #[test]
    fn slash_slash_and_block_comments_are_rejected_with_a_hint() {
        let result = lex("x // trailing\ny");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.to_string().contains("comments start with"))
        );
        // The rest of the line is skipped like a comment, so `y` still lexes.
        let idents: Vec<_> = result
            .tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::Ident(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(idents, vec!["x", "y"]);
    }

    #[test]
    fn unterminated_string_is_reported_not_panicked() {
        let result = lex("\"oops");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.to_string().contains("unterminated"))
        );
    }

    #[test]
    fn unknown_escape_names_the_valid_ones() {
        let result = lex(r#""a\qb""#);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.to_string().contains("unknown escape") && e.to_string().contains("\\n")),
            "got {:?}",
            result.errors
        );
    }

    #[test]
    fn curly_quotes_get_a_dedicated_hint() {
        let result = lex("\u{201C}alice\u{201D}");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.to_string().contains("curly quote")),
            "got {:?}",
            result.errors
        );
    }

    #[test]
    fn unicode_in_string_contents_is_preserved() {
        assert_eq!(kinds("\"café ☃\""), vec![TokenKind::Str("café ☃".into())]);
    }
}
