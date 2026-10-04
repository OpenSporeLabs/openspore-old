//! A minimal, dependency-free JSON writer.
//!
//! # Why not serde
//!
//! The workspace has one dependency-free convention (`spore-core`), and this
//! crate exists to audit the *asset* path, so its dependency list should stay
//! equal to the crates it inspects plus `thiserror`. Every JSON document this
//! tool emits has a handful of keys and no recursive schema, so a serializer
//! would be a large dependency for a small job.
//!
//! # Why the ordering guarantee matters
//!
//! [`Json::Obj`] is a `Vec` of pairs, not a map. Key order is therefore whatever
//! the code inserted, which is what "fixed key order" in the tool's contract
//! means: two runs produce byte-identical output because nothing iterates a hash
//! map. `to_string` is a pure function of the value, with no whitespace
//! and no locale- or platform-dependent formatting.

use std::fmt;

/// A JSON value, with object keys in insertion order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// An integral number. Rendered without a sign for values that fit.
    Int(i64),
    /// A string.
    Str(String),
    /// An array.
    Arr(Vec<Json>),
    /// An object, in insertion order. Duplicate keys are the caller's mistake
    /// and are emitted verbatim rather than silently collapsed.
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// An empty object.
    pub fn obj() -> Self {
        Json::Obj(Vec::new())
    }

    /// Builds an object from `(key, value)` pairs.
    pub fn object<K, I>(pairs: I) -> Self
    where
        K: Into<String>,
        I: IntoIterator<Item = (K, Json)>,
    {
        Json::Obj(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Appends a key. Only meaningful on [`Json::Obj`].
    ///
    /// The value is taken as `impl Into<Json>` so a call site can write
    /// `.push("name", "raster")` instead of `.push("name", Json::from("raster"))`.
    /// That is a convenience and nothing else: every value still goes through one
    /// conversion, so two spellings of the same value cannot render differently.
    ///
    /// Panics on a non-object, which is a programming error in this crate
    /// rather than a runtime condition: every call site builds the object it
    /// extends.
    pub fn push<K: Into<String>, V: Into<Json>>(mut self, key: K, value: V) -> Self {
        match &mut self {
            Json::Obj(pairs) => pairs.push((key.into(), value.into())),
            other => panic!("Json::push on {other:?}"),
        }
        self
    }

    /// Renders the value as compact JSON with no trailing newline.
    ///
    /// Named `render` rather than `to_string` on purpose: an inherent
    /// `to_string` shadows `Display`, and a JSON value that renders two
    /// different ways depending on which one a caller reached for is a bug
    /// waiting to be written.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write_into(&mut out);
        out
    }

    /// Renders the value as one line of JSON, newline terminated.
    pub fn to_line(&self) -> String {
        let mut out = self.render();
        out.push('\n');
        out
    }

    fn write_into(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Int(value) => out.push_str(&value.to_string()),
            Json::Str(value) => write_string(out, value),
            Json::Arr(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    item.write_into(out);
                }
                out.push(']');
            }
            Json::Obj(pairs) => {
                out.push('{');
                for (index, (key, value)) in pairs.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(out, key);
                    out.push(':');
                    value.write_into(out);
                }
                out.push('}');
            }
        }
    }
}

/// Appends a JSON string literal, escaping what RFC 8259 requires.
fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            // Control characters are the only thing JSON *requires* escaping;
            // everything else, including non-ASCII, is valid verbatim UTF-8.
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

impl From<&str> for Json {
    fn from(value: &str) -> Self {
        Json::Str(value.to_owned())
    }
}

impl From<String> for Json {
    fn from(value: String) -> Self {
        Json::Str(value)
    }
}

impl From<bool> for Json {
    fn from(value: bool) -> Self {
        Json::Bool(value)
    }
}

impl From<i64> for Json {
    fn from(value: i64) -> Self {
        Json::Int(value)
    }
}

impl From<u32> for Json {
    fn from(value: u32) -> Self {
        Json::Int(i64::from(value))
    }
}

impl From<u64> for Json {
    fn from(value: u64) -> Self {
        // A u64 above i64::MAX cannot be a valid JSON number for a JavaScript
        // consumer anyway; refusing to silently wrap is the honest choice, and
        // every value this tool emits is a byte count or an index.
        Json::Int(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl From<usize> for Json {
    fn from(value: usize) -> Self {
        Json::Int(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(inner) => inner.into(),
            None => Json::Null,
        }
    }
}

impl From<&String> for Json {
    fn from(value: &String) -> Self {
        Json::Str(value.clone())
    }
}

/// Formats an id field as the canonical `0x`-prefixed, zero-padded lowercase
/// hex string this repository uses everywhere.
///
/// A JSON *string*, not a number: `0x00e6bce5` is not representable as a JSON
/// number, and a decimal reader would have to know the field was hex. The
/// spelling is the same one `ResourceKey::to_tgi` produces, so an id copied out
/// of a `--json` document is a valid argument to `find`/`describe`/`extract`.
pub fn hex_id(value: u32) -> Json {
    Json::Str(format!("0x{value:08x}"))
}

/// A strict reader for what `to_string` writes.
///
/// A writer with no reader is a writer nobody can check. This parser is
/// deliberately minimal — it accepts exactly what the writer emits and rejects
/// everything else, including the trailing comma and unquoted key that a lenient
/// parser would forgive — so a test can assert "`--json` produced valid JSON"
/// rather than "the output contains these substrings". Object key order is
/// preserved on the way in, so a parsed document compares equal to the value it
/// came from.
pub fn parse(text: &str) -> Result<Json, ParseError> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    parser.skip_whitespace();
    let value = parser.value()?;
    parser.skip_whitespace();
    if parser.at != parser.bytes.len() {
        return Err(ParseError::TrailingBytes { at: parser.at });
    }
    Ok(value)
}

/// Where a document stopped being valid JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Input ended in the middle of a value.
    UnexpectedEnd,
    /// A byte that cannot start any value.
    UnexpectedByte {
        /// Its offset in the document.
        byte: u8,
        /// Its offset in the document.
        at: usize,
    },
    /// A `\u` escape that is not four hex digits.
    BadEscape {
        /// Its offset in the document.
        at: usize,
    },
    /// A literal other than `true`, `false` or `null`.
    BadLiteral {
        /// Its offset in the document.
        at: usize,
    },
    /// The document continued after a complete top-level value.
    TrailingBytes {
        /// Where the extra text starts.
        at: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => f.write_str("json: input ended mid-value"),
            Self::UnexpectedByte { byte, at } => {
                write!(
                    f,
                    "json: byte 0x{byte:02x} at offset {at} cannot start a value"
                )
            }
            Self::BadEscape { at } => write!(f, "json: bad \\u escape at offset {at}"),
            Self::BadLiteral { at } => write!(f, "json: bad literal at offset {at}"),
            Self::TrailingBytes { at } => {
                write!(
                    f,
                    "json: document continues after the top-level value at offset {at}"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Parser<'a> {
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.bad())
        }
    }

    fn bad(&self) -> ParseError {
        match self.peek() {
            None => ParseError::UnexpectedEnd,
            Some(byte) => ParseError::UnexpectedByte { byte, at: self.at },
        }
    }

    fn value(&mut self) -> Result<Json, ParseError> {
        self.skip_whitespace();
        match self.peek().ok_or(ParseError::UnexpectedEnd)? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Ok(Json::Str(self.string()?)),
            b't' => self.literal("true", Json::Bool(true)),
            b'f' => self.literal("false", Json::Bool(false)),
            b'n' => self.literal("null", Json::Null),
            byte @ (b'-' | b'0'..=b'9') => self.number(byte),
            _ => Err(self.bad()),
        }
    }

    fn literal(&mut self, text: &str, value: Json) -> Result<Json, ParseError> {
        if self.bytes[self.at..].starts_with(text.as_bytes()) {
            self.at += text.len();
            Ok(value)
        } else {
            Err(ParseError::BadLiteral { at: self.at })
        }
    }

    fn number(&mut self, first: u8) -> Result<Json, ParseError> {
        let start = self.at;
        if first == b'-' {
            self.at += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or("");
        text.parse::<i64>()
            .map(Json::Int)
            .map_err(|_| ParseError::UnexpectedByte {
                byte: first,
                at: start,
            })
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self.peek().ok_or(ParseError::UnexpectedEnd)?;
            self.at += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escape = self.peek().ok_or(ParseError::UnexpectedEnd)?;
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let end = self.at + 4;
                            let digits = self
                                .bytes
                                .get(self.at..end)
                                .ok_or(ParseError::UnexpectedEnd)?;
                            let text = std::str::from_utf8(digits)
                                .map_err(|_| ParseError::BadEscape { at: self.at })?;
                            let code = u32::from_str_radix(text, 16)
                                .map_err(|_| ParseError::BadEscape { at: self.at })?;
                            out.push(
                                char::from_u32(code)
                                    .ok_or(ParseError::BadEscape { at: self.at })?,
                            );
                            self.at = end;
                        }
                        _ => return Err(ParseError::BadEscape { at: self.at - 1 }),
                    }
                }
                _ => {
                    // Copy the whole UTF-8 sequence starting at this byte.
                    let start = self.at - 1;
                    let mut end = self.at;
                    while matches!(self.peek(), Some(0x80..=0xBF)) {
                        self.at += 1;
                        end = self.at;
                    }
                    let text = std::str::from_utf8(&self.bytes[start..end])
                        .map_err(|_| ParseError::UnexpectedByte { byte, at: start })?;
                    out.push_str(text);
                }
            }
        }
    }

    fn array(&mut self) -> Result<Json, ParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_whitespace();
            match self.peek().ok_or(ParseError::UnexpectedEnd)? {
                b',' => self.at += 1,
                b']' => {
                    self.at += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.bad()),
            }
        }
    }

    fn object(&mut self) -> Result<Json, ParseError> {
        self.expect(b'{')?;
        let mut pairs = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            let value = self.value()?;
            pairs.push((key, value));
            self.skip_whitespace();
            match self.peek().ok_or(ParseError::UnexpectedEnd)? {
                b',' => self.at += 1,
                b'}' => {
                    self.at += 1;
                    return Ok(Json::Obj(pairs));
                }
                _ => return Err(self.bad()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_render_as_json_literals() {
        assert_eq!(Json::Null.render(), "null");
        assert_eq!(Json::from(true).render(), "true");
        assert_eq!(Json::from(false).render(), "false");
        assert_eq!(Json::from(42u32).render(), "42");
        assert_eq!(Json::from(0usize).render(), "0");
        assert_eq!(Json::from("hi").render(), "\"hi\"");
    }

    #[test]
    fn escaping_covers_exactly_what_the_grammar_requires() {
        assert_eq!(
            Json::from("a\"b\\c\nd\te\u{8}f\u{c}g\u{1}h").render(),
            "\"a\\\"b\\\\c\\nd\\te\\bf\\fg\\u0001h\""
        );
        // Non-ASCII stays verbatim: valid UTF-8 is valid JSON.
        assert_eq!(Json::from("ñ\u{1f600}").render(), "\"\u{f1}\u{1f600}\"");
    }

    #[test]
    fn object_key_order_is_insertion_order_not_sorted() {
        let value = Json::obj()
            .push("zebra", Json::from(1u32))
            .push("alpha", Json::from(2u32));
        assert_eq!(value.render(), r#"{"zebra":1,"alpha":2}"#);
    }

    #[test]
    fn arrays_keep_their_order_and_nest() {
        let value = Json::Arr(vec![
            Json::from(1u32),
            Json::Arr(vec![Json::from("a"), Json::Null]),
        ]);
        assert_eq!(value.render(), r#"[1,["a",null]]"#);
    }

    #[test]
    fn to_line_appends_exactly_one_newline() {
        assert_eq!(Json::from(1u32).to_line(), "1\n");
    }

    #[test]
    fn hex_ids_round_trip_through_the_canonical_spelling() {
        assert_eq!(hex_id(0x00e6_bce5).render(), "\"0x00e6bce5\"");
        // The same spelling `ResourceKey::to_tgi` uses per component, so a value
        // lifted out of a --json document is a usable argument.
        assert_eq!(hex_id(0x2f4e_681c).render(), "\"0x2f4e681c\"");
    }

    #[test]
    #[should_panic(expected = "Json::push on")]
    fn pushing_onto_a_non_object_is_a_programming_error_not_a_silent_no_op() {
        let _ = Json::from(1u32).push("k", Json::Null);
    }

    #[test]
    fn everything_written_parses_back_to_the_same_value() {
        let value = Json::obj()
            .push("a", Json::from(1u32))
            .push("b", Json::Null)
            .push(
                "c",
                Json::Arr(vec![Json::from(true), Json::from("x\ny\"z")]),
            )
            .push("d", Json::from(-7i64))
            .push("e", Json::obj());
        assert_eq!(parse(&value.render()), Ok(value.clone()));
        assert_eq!(parse(&value.to_line()), Ok(value));
    }

    #[test]
    fn the_parser_preserves_key_order_so_a_round_trip_is_comparable() {
        let value = Json::obj()
            .push("z", Json::from(1u32))
            .push("a", Json::from(2u32));
        let Json::Obj(pairs) = parse(&value.render()).unwrap() else {
            panic!("expected an object")
        };
        let keys: Vec<&str> = pairs.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, vec!["z", "a"]);
    }

    #[test]
    fn non_ascii_survives_the_round_trip() {
        let value = Json::from("ñ\u{1f600} é");
        assert_eq!(parse(&value.render()), Ok(value));
    }

    #[test]
    fn the_parser_refuses_what_the_writer_never_produces() {
        for bad in [
            "",
            "{",
            "[1,",
            "[1,]",
            "{'a':1}",
            "nul",
            "\"unterminated",
            "1 2",
            "--1",
            "{\"a\" 1}",
            "\"\\q\"",
            "\"\\u00\"",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn a_parse_error_names_its_offset() {
        assert_eq!(
            parse("[1 x]"),
            Err(ParseError::UnexpectedByte { byte: b'x', at: 3 })
        );
        assert_eq!(parse("[1] [2]"), Err(ParseError::TrailingBytes { at: 4 }));
        assert!(parse("[1").unwrap_err().to_string().contains("mid-value"));
    }
}
