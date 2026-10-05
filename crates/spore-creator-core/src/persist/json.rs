//! The JSON this format speaks: a deterministic writer and a fail-closed reader.
//!
//! It is **not** a general-purpose JSON library and does not try to be. It
//! exists for one artifact shape, and two of its choices are load-bearing:
//!
//! * The writer ([`Out`]) emits keys in the order the caller asks for them and
//!   holds no map, so the byte sequence is a pure function of the document.
//!   There is no iteration order to get wrong, which is the whole determinism
//!   story: no `HashMap`, no `BTreeMap`, no sort, nothing to forget.
//! * The reader ([`parse`]) is **fail-closed**. An unknown field, a duplicate
//!   field, a trailing byte after the value, a control character inside a string
//!   — each is an error naming its line, never a skipped child. This mirrors
//!   [`crate::xml`] and it is for the same reason: a field this build does not
//!   understand has to be a deliberate decision to adopt, not a silent loss.
//!
//! # Numbers keep their literal text
//!
//! [`Json::Number`] holds the number **as written**, not as an `f64`. That is not
//! pedantry: `"1e40".parse::<f32>()` is `Ok(f32::INFINITY)`, so a JSON number
//! that has not been read back and checked is a number this format cannot
//! accept. Keeping the text is what lets the float reader refuse that instead of
//! quietly widening a creator document to infinity.
//!
//! Strings are decoded on the way in — escapes resolved, surrogate pairs
//! combined — so no consumer downstream has to know about `\uXXXX`.

use crate::document::BlockIndex;
use crate::error::{PersistError, PersistSite};

// ---------------------------------------------------------------------------
// values
// ---------------------------------------------------------------------------

/// One JSON value, as read from a line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    /// `null`. Used inside fixed-width arrays, where a hole cannot be spelled
    /// as an absent key.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// A number, held as the literal text it was written with.
    Number(String),
    /// A string, with escapes already resolved.
    String(String),
    /// An array.
    Array(Vec<Json>),
    /// An object, in written order.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// The closed-vocabulary name of this value's JSON type, for error messages.
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }
}

/// Where a value came from, so every failure can name it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx {
    /// 1-based line number in the file.
    pub line: usize,
    /// The block the value belongs to, when it is inside one.
    pub block: Option<BlockIndex>,
}

impl Ctx {
    pub(crate) fn document(line: usize) -> Self {
        Self { line, block: None }
    }
}

/// Both conversions are total and lossless, so a caller can move a location
/// between this module's reader vocabulary and the public error type without
/// being asked which direction a `site` is in.
impl From<Ctx> for PersistSite {
    fn from(ctx: Ctx) -> Self {
        Self {
            line: ctx.line,
            block: ctx.block,
        }
    }
}

impl From<PersistSite> for Ctx {
    fn from(site: PersistSite) -> Self {
        Self {
            line: site.line,
            block: site.block,
        }
    }
}

// ---------------------------------------------------------------------------
// reading
// ---------------------------------------------------------------------------

/// Parses one line as exactly one JSON value, with nothing after it.
pub(crate) fn parse(text: &str, line: usize) -> Result<Json, PersistError> {
    let mut reader = Reader {
        text: text.as_bytes(),
        at: 0,
        line,
    };
    reader.skip_spaces();
    let value = reader.value()?;
    reader.skip_spaces();
    if reader.at != reader.text.len() {
        return Err(reader.syntax("trailing content after the JSON value"));
    }
    Ok(value)
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
    line: usize,
}

impl Reader<'_> {
    fn syntax(&self, reason: &'static str) -> PersistError {
        PersistError::Syntax {
            site: PersistSite::document(self.line),
            reason: reason.to_owned(),
        }
    }

    fn skip_spaces(&mut self) {
        while matches!(self.text.get(self.at), Some(b' ' | b'\t')) {
            self.at += 1;
        }
    }

    /// Only the four whitespace characters JSON allows. Notably **not** `\n`,
    /// `\r` or `\0`: a line-delimited artifact must not carry a stray carriage
    /// return, because that would make the same document hash differently on a
    /// checkout that added one.
    fn skip_whitespace(&mut self) {
        while matches!(self.text.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Result<u8, PersistError> {
        self.text
            .get(self.at)
            .copied()
            .ok_or_else(|| self.syntax("the value ended early"))
    }

    fn literal(&mut self, word: &str) -> Result<(), PersistError> {
        if self.text[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(())
        } else {
            Err(self.syntax("an unrecognised literal"))
        }
    }

    fn value(&mut self) -> Result<Json, PersistError> {
        match self.peek()? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Ok(Json::String(self.string()?)),
            b't' => {
                self.literal("true")?;
                Ok(Json::Bool(true))
            }
            b'f' => {
                self.literal("false")?;
                Ok(Json::Bool(false))
            }
            b'n' => {
                self.literal("null")?;
                Ok(Json::Null)
            }
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(self.syntax("not the start of a JSON value")),
        }
    }

    fn object(&mut self) -> Result<Json, PersistError> {
        self.at += 1; // '{'
        let mut entries: Vec<(String, Json)> = Vec::new();
        self.skip_whitespace();
        if self.text.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_whitespace();
            if self.peek()? != b'"' {
                return Err(self.syntax("an object key must be a quoted string"));
            }
            let key = self.string()?;
            // A duplicate key makes "what does this field say" ambiguous, so it
            // is refused here rather than resolved by last-one-wins.
            if entries.iter().any(|(seen, _)| *seen == key) {
                return Err(self.syntax("a field appears twice in one object"));
            }
            self.skip_whitespace();
            if self.peek()? != b':' {
                return Err(self.syntax("an object key must be followed by `:`"));
            }
            self.at += 1;
            self.skip_whitespace();
            let value = self.value()?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.peek()? {
                b',' => self.at += 1,
                b'}' => {
                    self.at += 1;
                    return Ok(Json::Object(entries));
                }
                _ => return Err(self.syntax("an object needs `,` or `}` after a field")),
            }
        }
    }

    fn array(&mut self) -> Result<Json, PersistError> {
        self.at += 1; // '['
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.text.get(self.at) == Some(&b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value()?);
            self.skip_whitespace();
            match self.peek()? {
                b',' => self.at += 1,
                b']' => {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.syntax("an array needs `,` or `]` after an item")),
            }
        }
    }

    /// Reads a JSON string with its escapes resolved.
    fn string(&mut self) -> Result<String, PersistError> {
        self.at += 1; // opening quote
        let mut out = String::new();
        loop {
            let byte = self.peek()?;
            match byte {
                b'"' => {
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.at += 1;
                    self.escape(&mut out)?;
                }
                0x00..=0x1f => {
                    return Err(self.syntax("a raw control character inside a string"));
                }
                _ => {
                    // Copy one whole UTF-8 sequence. The input came from a
                    // `&str`, so the bytes are already known-valid UTF-8 and the
                    // continuation-byte check below cannot fail.
                    let start = self.at;
                    self.at += utf8_width(byte);
                    let slice = self.text.get(start..self.at).ok_or_else(|| {
                        self.syntax("a string ends in the middle of a UTF-8 sequence")
                    })?;
                    let text = core::str::from_utf8(slice)
                        .map_err(|_| self.syntax("a string is not valid UTF-8"))?;
                    out.push_str(text);
                }
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), PersistError> {
        let byte = self.peek()?;
        self.at += 1;
        let plain = match byte {
            b'"' => Some('"'),
            b'\\' => Some('\\'),
            b'/' => Some('/'),
            b'b' => Some('\u{8}'),
            b'f' => Some('\u{c}'),
            b'n' => Some('\n'),
            b'r' => Some('\r'),
            b't' => Some('\t'),
            b'u' => None,
            _ => return Err(self.syntax("`\\` must be followed by a JSON escape")),
        };
        if let Some(plain) = plain {
            out.push(plain);
            return Ok(());
        }
        let unit = self.hex4()?;
        // A high surrogate must be followed by `\u` low, and the pair is one
        // scalar value; a lone surrogate is not a `char` and is refused rather
        // than replaced, which would be a silent substitution.
        let ch = if (0xd800..0xdc00).contains(&unit) {
            if self.text[self.at..].starts_with(br"\u") {
                self.at += 2;
                let low = self.hex4()?;
                if !(0xdc00..0xe000).contains(&low) {
                    return Err(self.syntax("a high surrogate is not followed by a low one"));
                }
                let combined = 0x1_0000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
                char::from_u32(combined).ok_or_else(|| self.syntax("not a Unicode scalar value"))?
            } else {
                return Err(self.syntax("a lone high surrogate is not a character"));
            }
        } else if (0xdc00..0xe000).contains(&unit) {
            return Err(self.syntax("a lone low surrogate is not a character"));
        } else {
            char::from_u32(unit).ok_or_else(|| self.syntax("not a Unicode scalar value"))?
        };
        out.push(ch);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, PersistError> {
        let digits = self
            .text
            .get(self.at..self.at + 4)
            .ok_or_else(|| self.syntax("`\\u` needs four hex digits"))?;
        let text =
            core::str::from_utf8(digits).map_err(|_| self.syntax("`\\u` needs four hex digits"))?;
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(self.syntax("`\\u` needs four hex digits"));
        }
        self.at += 4;
        u32::from_str_radix(text, 16).map_err(|_| self.syntax("`\\u` needs four hex digits"))
    }

    /// Reads a number and keeps its text, per this module's note.
    ///
    /// The grammar is RFC 8259's exactly: an optional `-`, then `0` or a
    /// non-zero-leading digit run, then an optional fraction, then an optional
    /// exponent. That excludes `NaN`, `inf`, a leading `+`, a leading zero
    /// (`01`), a bare `.5` and a trailing `5.` — every one of which Rust's
    /// `f32::from_str` would happily accept, so the check has to happen here.
    fn number(&mut self) -> Result<Json, PersistError> {
        let start = self.at;
        if self.text.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        self.digits()?;
        if self.text.get(self.at) == Some(&b'.') {
            self.at += 1;
            self.digits()?;
        }
        if matches!(self.text.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.text.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.digits()?;
        }
        let text = core::str::from_utf8(&self.text[start..self.at])
            .map_err(|_| self.syntax("a number is not valid UTF-8"))?;
        Ok(Json::Number(text.to_owned()))
    }

    fn digits(&mut self) -> Result<(), PersistError> {
        let start = self.at;
        while matches!(self.text.get(self.at), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        if self.at == start {
            return Err(self.syntax("a number needs at least one digit"));
        }
        if self.at - start > 1 && self.text[start] == b'0' {
            return Err(self.syntax("a number may not have a leading zero"));
        }
        Ok(())
    }
}

/// How many bytes the UTF-8 sequence starting with `first` occupies.
fn utf8_width(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

// ---------------------------------------------------------------------------
// typed access
// ---------------------------------------------------------------------------

/// One object's fields, **taken** as they are read.
///
/// The point of taking rather than borrowing: [`Fields::finish`] then has a
/// complete answer to "is there anything left?", and "yes" is either a field
/// this build does not know or a field the reader forgot. Both are errors, and
/// neither can be reached by a caller that forgot to handle a field.
#[derive(Debug)]
pub(crate) struct Fields {
    entries: Vec<Option<(String, Json)>>,
    ctx: Ctx,
}

impl Fields {
    /// Opens the object at `path`, refusing anything that is not one.
    ///
    /// `path` is carried only so that a non-object here is reported as *which*
    /// field was not an object; the fields inside are named as they are taken.
    pub(crate) fn open(value: Json, ctx: Ctx, path: &str) -> Result<Self, PersistError> {
        match value {
            Json::Object(entries) => Ok(Self {
                entries: entries.into_iter().map(Some).collect(),
                ctx,
            }),
            other => Err(PersistError::WrongType {
                site: ctx.into(),
                field: path.to_owned(),
                expected: "object".to_owned(),
                found: other.type_name().to_owned(),
            }),
        }
    }

    /// Takes the named field, or `None` when the object does not have it.
    pub(crate) fn take(&mut self, field: &str) -> Option<Json> {
        self.entries
            .iter_mut()
            .find(|entry| entry.as_ref().is_some_and(|(key, _)| key == field))
            .and_then(Option::take)
            .map(|(_, value)| value)
    }

    /// Takes the named field, which the grammar requires.
    pub(crate) fn require(&mut self, field: &str) -> Result<Json, PersistError> {
        self.take(field).ok_or_else(|| PersistError::MissingField {
            site: self.ctx.into(),
            field: field.to_owned(),
        })
    }

    /// Requires that nothing is left over.
    pub(crate) fn finish(self) -> Result<(), PersistError> {
        if let Some((key, _)) = self.entries.into_iter().flatten().next() {
            return Err(PersistError::UnexpectedField {
                site: self.ctx.into(),
                field: key,
            });
        }
        Ok(())
    }
}

/// Reads a `u32` word: the repo's canonical `0x` hex spelling, or a bare decimal.
pub(crate) fn word(ctx: Ctx, field: &str, value: &Json) -> Result<u32, PersistError> {
    let Json::String(text) = value else {
        return Err(wrong_type(ctx, field, "string", value));
    };
    // `spore_core::parse_id`, never a second hex parser: the class of bug this
    // repository has already paid for twice. It accepts either radix, which is
    // why the *reader* is lenient and the *writer* is not.
    spore_core::parse_id(text).map_err(|_| PersistError::NotAnId {
        site: ctx.into(),
        field: field.to_owned(),
        value: text.clone(),
    })
}

/// Reads a positional index, which is decimal and is **not** a word.
///
/// [`crate::document::BlockIndex`] is a position in `blocks`, not an id, so it
/// is spelled the way a position is spelled. Writing it as hex would make a
/// child index indistinguishable from the id it points at.
pub(crate) fn index(ctx: Ctx, field: &str, value: &Json) -> Result<usize, PersistError> {
    let Json::Number(text) = value else {
        return Err(wrong_type(ctx, field, "number", value));
    };
    text.parse::<usize>()
        .ok()
        .filter(|_| text.bytes().all(|b| b.is_ascii_digit()) && !text.is_empty())
        .ok_or_else(|| PersistError::NotAnIndex {
            site: ctx.into(),
            field: field.to_owned(),
            value: text.clone(),
        })
}

/// Reads a fixed-width list, refusing any other arity.
pub(crate) fn list<const N: usize>(
    ctx: Ctx,
    field: &str,
    value: &Json,
) -> Result<Vec<Json>, PersistError> {
    let Json::Array(items) = value else {
        return Err(wrong_type(ctx, field, "array", value));
    };
    if items.len() != N {
        return Err(PersistError::WrongArity {
            site: ctx.into(),
            field: field.to_owned(),
            expected: N,
            got: items.len(),
        });
    }
    Ok(items.clone())
}

/// Reads a variable-length list.
pub(crate) fn items(ctx: Ctx, field: &str, value: &Json) -> Result<Vec<Json>, PersistError> {
    match value {
        Json::Array(items) => Ok(items.clone()),
        other => Err(wrong_type(ctx, field, "array", other)),
    }
}

/// Reads a string.
pub(crate) fn text(ctx: Ctx, field: &str, value: &Json) -> Result<String, PersistError> {
    match value {
        Json::String(text) => Ok(text.clone()),
        other => Err(wrong_type(ctx, field, "string", other)),
    }
}

/// Reads a flag.
pub(crate) fn flag(ctx: Ctx, field: &str, value: &Json) -> Result<bool, PersistError> {
    match value {
        Json::Bool(flag) => Ok(*flag),
        other => Err(wrong_type(ctx, field, "bool", other)),
    }
}

pub(crate) fn wrong_type(ctx: Ctx, field: &str, expected: &str, found: &Json) -> PersistError {
    PersistError::WrongType {
        site: ctx.into(),
        field: field.to_owned(),
        expected: expected.to_owned(),
        found: found.type_name().to_owned(),
    }
}

// ---------------------------------------------------------------------------
// writing
// ---------------------------------------------------------------------------

/// A deterministic JSON writer: no map, no iteration, no reordering.
#[derive(Debug, Default)]
pub(crate) struct Out {
    text: String,
    /// One entry per open container: has a value been written yet, and what
    /// closes it.
    open: Vec<(bool, u8)>,
    /// A key has been written and its value has not, so the value must not
    /// introduce a comma.
    pending_value: bool,
}

impl Out {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Separates this value from the previous one in the same container.
    fn before_value(&mut self) {
        if core::mem::take(&mut self.pending_value) {
            return;
        }
        if let Some((written, _)) = self.open.last_mut() {
            if *written {
                self.text.push(',');
            }
            *written = true;
        }
    }

    /// Opens a container, closing with `close`.
    pub(crate) fn container(&mut self, open: char, close: char) {
        self.before_value();
        self.text.push(open);
        self.open.push((false, close as u8));
    }

    pub(crate) fn object(&mut self) {
        self.container('{', '}');
    }

    pub(crate) fn array(&mut self) {
        self.container('[', ']');
    }

    /// Closes the innermost container, which must be the one `close` names.
    ///
    /// A mismatched closer is a bug in the encoder rather than in the data, and
    /// a `debug_assert` is the right weight for it: the crate is
    /// `#![forbid(unsafe_code)]` and every write below is hand-balanced, so a
    /// mistake shows up as a malformed line that [`parse`] refuses, not as
    /// memory trouble.
    pub(crate) fn end(&mut self, close: char) {
        let (_, expected) = self.open.pop().expect("balanced by the encoder");
        debug_assert_eq!(expected, close as u8, "unbalanced container");
        self.text.push(close);
    }

    pub(crate) fn key(&mut self, name: &str) {
        self.before_value();
        self.quoted(name);
        self.text.push(':');
        self.pending_value = true;
    }

    pub(crate) fn null(&mut self) {
        self.before_value();
        self.text.push_str("null");
    }

    pub(crate) fn flag(&mut self, value: bool) {
        self.before_value();
        self.text.push_str(if value { "true" } else { "false" });
    }

    /// Writes a positional index, in decimal.
    pub(crate) fn index(&mut self, value: usize) {
        self.before_value();
        let mut digits = [0u8; 20];
        let mut at = digits.len();
        let mut value = value;
        loop {
            at -= 1;
            digits[at] = b'0' + u8::try_from(value % 10).expect("a decimal digit");
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.text
            .push_str(core::str::from_utf8(&digits[at..]).expect("ascii digits"));
    }

    pub(crate) fn string(&mut self, value: &str) {
        self.before_value();
        self.quoted(value);
    }

    /// Writes a quoted, escaped string, quotes included.
    fn quoted(&mut self, value: &str) {
        self.text.push('"');
        for ch in value.chars() {
            match ch {
                '"' => self.text.push_str("\\\""),
                '\\' => self.text.push_str("\\\\"),
                '\n' => self.text.push_str("\\n"),
                '\r' => self.text.push_str("\\r"),
                '\t' => self.text.push_str("\\t"),
                '\u{8}' => self.text.push_str("\\b"),
                '\u{c}' => self.text.push_str("\\f"),
                // Control characters only: the rest of Unicode is written as
                // itself, which is valid JSON and keeps a creature's name
                // readable in the artifact.
                ch if (ch as u32) < 0x20 => {
                    self.text.push_str(&format!("\\u{:04x}", ch as u32));
                }
                ch => self.text.push(ch),
            }
        }
        self.text.push('"');
    }

    /// Ends the current line.
    pub(crate) fn line(&mut self) {
        self.text.push('\n');
    }

    pub(crate) fn finish(self) -> String {
        debug_assert!(self.open.is_empty(), "an unclosed container");
        self.text
    }

    /// Empties the buffer so an exhaustive sweep does not re-allocate it 2^32
    /// times. Test-only: production writes one value per document.
    #[cfg(test)]
    pub(crate) fn take(&mut self) -> String {
        debug_assert!(self.open.is_empty(), "an unclosed container");
        core::mem::take(&mut self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Json, PersistError> {
        parse(text, 1)
    }

    #[test]
    fn a_written_value_reads_back_identically() {
        for text in [
            "null",
            "true",
            "false",
            "0",
            "-0.5",
            "1e-40",
            "\"\"",
            "[]",
            "{}",
            "[1,2,3]",
            "{\"a\":1,\"b\":[true,null]}",
            r#""A creature is named \"something\".""#,
            r#""tab\there""#,
            "[]",
        ] {
            assert_eq!(read(text).unwrap(), parse(text, 1).unwrap(), "{text}");
        }
    }

    #[test]
    fn a_string_keeps_its_escapes_resolved() {
        assert_eq!(
            read(r#""aé😀b""#).unwrap(),
            Json::String("aé😀b".to_owned())
        );
        assert_eq!(read(r#""\/""#).unwrap(), Json::String("/".to_owned()));
    }

    #[test]
    fn a_lone_surrogate_is_refused_not_replaced() {
        // U+FFFD would be a silent substitution of one character for another.
        assert!(read(r#""\ud800""#).is_err());
        assert!(read(r#""\udc00""#).is_err());
        assert!(read(r#""\ud800A""#).is_err());
        assert!(read(r#""\ud83dA""#).is_err());
        // A well-formed pair is one character, not two.
        assert_eq!(
            read(r#""\ud83d\ude00""#).unwrap(),
            Json::String("\u{1f600}".to_owned())
        );
    }

    #[test]
    fn the_number_grammar_is_jsons_and_not_rusts() {
        // All of these parse in Rust and none of them is a JSON number.
        for text in [
            "01", "+1", ".5", "5.", "NaN", "inf", "-inf", "1e", "1e+", "0x10", "1.2.3",
        ] {
            assert!(read(text).is_err(), "{text:?} must be refused");
        }
        assert_eq!(
            read("1e-40").unwrap(),
            Json::Number("1e-40".to_owned()),
            "the text is kept exactly as written"
        );
    }

    #[test]
    fn a_duplicate_field_is_an_error_and_not_last_one_wins() {
        assert!(read("{\"a\":1,\"a\":2}").is_err());
    }

    #[test]
    fn trailing_content_after_a_value_is_refused() {
        assert!(read("{} {}").is_err());
        assert!(read("1,").is_err());
        assert!(read("").is_err());
    }

    #[test]
    fn a_raw_control_character_in_a_string_is_refused() {
        assert!(parse("{\"a\":\"x\ny\"}", 1).is_err());
        assert!(parse("{\"a\":\"x\u{1}y\"}", 1).is_err());
    }

    #[test]
    fn the_writer_emits_keys_in_the_order_it_is_asked_and_adds_no_whitespace() {
        let mut out = Out::new();
        out.object();
        out.key("b");
        out.index(2);
        out.key("a");
        out.array();
        out.flag(true);
        out.index(0);
        out.end(']');
        out.key("empty");
        out.array();
        out.end(']');
        out.end('}');
        assert_eq!(out.finish(), r#"{"b":2,"a":[true,0],"empty":[]}"#);
    }

    #[test]
    fn what_the_writer_emits_parses_back_to_the_same_value() {
        let mut out = Out::new();
        out.object();
        out.key("name");
        out.string("a \"quoted\" \\ name\nwith a tab\tand a control ");
        out.key("deep");
        out.array();
        out.object();
        out.key("x");
        out.string("-0.0");
        out.end('}');
        out.end(']');
        out.end('}');
        let text = out.finish();
        let value = read(&text).unwrap();
        let Json::Object(entries) = &value else {
            panic!("an object");
        };
        assert_eq!(entries[0].0, "name");
        assert_eq!(entries[1].1, read(r#"[{"x":"-0.0"}]"#).unwrap());
    }
}
