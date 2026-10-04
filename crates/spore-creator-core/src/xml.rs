//! A deliberately small XML reader.
//!
//! The 68 creator documents parse with any conformant XML parser, so this is
//! **not** a general XML implementation and must not grow into one. It reads the
//! subset the corpus uses and **fails closed** on everything else, because the
//! alternative — skipping what it does not recognise — is how a decoder starts
//! reporting facts about a document it never fully read.
//!
//! Accepted: an optional XML declaration, elements, attributes, character data,
//! comments, self-closing elements, and the five predefined entities plus
//! numeric character references.
//!
//! Refused, loudly: namespaces, CDATA sections, DOCTYPE, processing
//! instructions, mixed content that is not whitespace between elements, and any
//! element or attribute outside the grammar the caller declares.

use crate::error::{CreatorError, Result};

/// One element, with its attributes and its character data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// The tag name.
    pub name: String,
    /// Attributes in written order. Order is preserved because the corpus's
    /// `count` attributes are load-bearing and a reader that reordered them
    /// would lose the document's own ordering.
    pub attributes: Vec<(String, String)>,
    /// Concatenated character data directly inside this element.
    pub text: String,
    /// Child elements in written order.
    pub children: Vec<Element>,
}

impl Element {
    /// The value of an attribute.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The first child with this name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.name == name)
    }

    /// Every child with this name, in written order.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |child| child.name == name)
    }

    /// The trimmed text of the first child with this name.
    ///
    /// `None` when there is no such child — which is a different answer from a
    /// child that exists and is empty, and the corpus needs both.
    pub fn child_text(&self, name: &str) -> Option<&str> {
        self.child(name).map(|child| child.text.trim())
    }
}

/// Parses one XML document and returns its root element.
pub fn parse(input: &str) -> Result<Element> {
    Parser {
        bytes: input.as_bytes(),
        pos: 0,
    }
    .document()
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn error(&self, reason: impl Into<String>) -> CreatorError {
        CreatorError::Xml {
            reason: reason.into(),
            offset: self.pos,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.bytes[self.pos..].starts_with(prefix.as_bytes())
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }

    /// Consumes whitespace plus comments, which is all that may sit between
    /// elements in this subset.
    fn skip_ignorable(&mut self) -> Result<()> {
        loop {
            self.skip_whitespace();
            if self.starts_with("<!--") {
                let end = find(self.bytes, self.pos + 4, b"-->")
                    .ok_or_else(|| self.error("unterminated comment"))?;
                self.pos = end + 3;
                continue;
            }
            return Ok(());
        }
    }

    fn document(&mut self) -> Result<Element> {
        self.skip_ignorable()?;
        if self.starts_with("<?xml") {
            let end = find(self.bytes, self.pos, b"?>")
                .ok_or_else(|| self.error("unterminated xml declaration"))?;
            if self.bytes[self.pos..end].contains(&b'!') {
                return Err(self.error("a doctype is not read by this subset"));
            }
            self.pos = end + 2;
            self.skip_ignorable()?;
        }
        let root = self.element()?;
        self.skip_ignorable()?;
        if self.pos != self.bytes.len() {
            return Err(self.error("trailing content after the root element"));
        }
        Ok(root)
    }

    fn element(&mut self) -> Result<Element> {
        if self.peek() != Some(b'<') {
            return Err(self.error("expected `<`"));
        }
        self.pos += 1;
        let name = self.name()?;
        let mut element = Element {
            name: name.clone(),
            ..Element::default()
        };
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'/') => {
                    self.pos += 1;
                    if self.peek() != Some(b'>') {
                        return Err(self.error("expected `>` after `/`"));
                    }
                    self.pos += 1;
                    return Ok(element);
                }
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(_) => {
                    let attribute = self.attribute()?;
                    if element
                        .attributes
                        .iter()
                        .any(|(key, _)| *key == attribute.0)
                    {
                        return Err(self.error(format!("repeated attribute {}", attribute.0)));
                    }
                    element.attributes.push(attribute);
                }
                None => return Err(self.error("unterminated start tag")),
            }
        }

        // Content: character data and child elements, never mixed.
        loop {
            match self.peek() {
                None => return Err(self.error(format!("unterminated <{name}>"))),
                Some(b'<') => {
                    if self.starts_with("</") {
                        self.pos += 2;
                        let closing = self.name()?;
                        if closing != name {
                            return Err(self.error(format!("<{name}> is closed by </{closing}>")));
                        }
                        self.skip_whitespace();
                        if self.peek() != Some(b'>') {
                            return Err(self.error("expected `>` to close an end tag"));
                        }
                        self.pos += 1;
                        return Ok(element);
                    }
                    if self.starts_with("<![CDATA[") {
                        return Err(self.error("CDATA is not read by this subset"));
                    }
                    if self.starts_with("<?") {
                        return Err(self.error("processing instructions are not read"));
                    }
                    if self.starts_with("<!") {
                        return Err(self.error("declarations are not read by this subset"));
                    }
                    element.children.push(self.element()?);
                }
                Some(_) => {
                    let text = self.text()?;
                    if !text.trim().is_empty() {
                        // Significant text beside a child element is content
                        // this subset does not model. Whitespace is not: every
                        // corpus document is indented.
                        if !element.children.is_empty() {
                            return Err(
                                self.error(format!("<{name}> mixes text and child elements"))
                            );
                        }
                        element.text.push_str(&text);
                    }
                }
            }
        }
    }

    fn name(&mut self) -> Result<String> {
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':') {
                self.pos += 1;
            } else {
                break;
            }
        }
        if start == self.pos {
            return Err(self.error("expected a name"));
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .map(str::to_owned)
            .map_err(|_| self.error("name is not utf-8"))
    }

    fn attribute(&mut self) -> Result<(String, String)> {
        let name = self.name()?;
        if name.contains(':') {
            return Err(self.error("namespaced attributes are not read"));
        }
        self.skip_whitespace();
        if self.peek() != Some(b'=') {
            return Err(self.error(format!("attribute {name} has no value")));
        }
        self.pos += 1;
        self.skip_whitespace();
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(self.error(format!("attribute {name} value is not quoted"))),
        };
        self.pos += 1;
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if byte == quote {
                let raw = std::str::from_utf8(&self.bytes[start..self.pos])
                    .map_err(|_| self.error("attribute value is not utf-8"))?;
                self.pos += 1;
                return Ok((name, unescape(raw)));
            }
            if byte == b'<' {
                return Err(self.error(format!("attribute {name} value contains `<`")));
            }
            self.pos += 1;
        }
        Err(self.error(format!("attribute {name} value is unterminated")))
    }

    fn text(&mut self) -> Result<String> {
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if byte == b'<' {
                break;
            }
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.error("character data is not utf-8"))?;
        Ok(unescape(raw))
    }
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|index| index + from)
}

/// Expands the five predefined entities and numeric character references.
///
/// An unknown entity is left as written rather than dropped: silently deleting
/// text would change a value nobody could then audit.
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let entity = &rest[1..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let decoded = entity
                    .strip_prefix('#')
                    .and_then(|digits| match digits.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => digits.parse::<u32>().ok(),
                    })
                    .and_then(char::from_u32);
                match decoded {
                    Some(c) => out.push(c),
                    None => {
                        out.push('&');
                        out.push_str(entity);
                        out.push(';');
                    }
                }
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minimal_document_parses() {
        let root = parse("<a><b>1</b><c/></a>").unwrap();
        assert_eq!(root.name, "a");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.child_text("b"), Some("1"));
        assert!(root.child("c").unwrap().children.is_empty());
    }

    #[test]
    fn attributes_keep_written_order_and_repeats_are_refused() {
        let root = parse(r#"<a count="8" flag="x"><b/></a>"#).unwrap();
        assert_eq!(root.attribute("count"), Some("8"));
        assert_eq!(
            root.attributes
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            vec!["count", "flag"]
        );
        assert!(parse(r#"<a x="1" x="2"/>"#).is_err());
    }

    #[test]
    fn a_missing_attribute_is_absent_not_empty() {
        let root = parse("<a><b/></a>").unwrap();
        assert_eq!(root.attribute("count"), None);
        assert_eq!(root.child_text("b"), Some(""));
        assert_eq!(root.child_text("nope"), None);
    }

    #[test]
    fn the_declaration_and_comments_are_skipped() {
        let root = parse("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!-- hi --><a>x</a>").unwrap();
        assert_eq!(root.name, "a");
        assert_eq!(root.text, "x");
    }

    #[test]
    fn constructs_this_subset_refuses_are_all_errors_not_skips() {
        for input in [
            "<a><![CDATA[raw]]></a>",
            "<a><?pi?></a>",
            "<!DOCTYPE a><a/>",
            "<a xmlns:n='u'/>",
            "<a><b>text</b>more</a>",
            "<a>",
            "<a></b>",
            "<a/><b/>",
            "<a x=1/>",
            "<a><!-- unterminated",
        ] {
            assert!(parse(input).is_err(), "{input} should be refused");
        }
    }

    #[test]
    fn entities_expand_and_an_unknown_one_is_left_visible() {
        let root = parse("<a>1 &lt; 2 &amp;&#65; &nope;</a>").unwrap();
        assert_eq!(root.text, "1 < 2 &A &nope;");
    }

    #[test]
    fn whitespace_between_elements_is_not_content() {
        let root = parse("<a>\n  <b>1</b>\n  <b>2</b>\n</a>").unwrap();
        assert_eq!(root.text, "");
        assert_eq!(root.children_named("b").count(), 2);
    }

    #[test]
    fn an_error_carries_the_offset_it_gave_up_at() {
        let error = parse("<a><b></a>").unwrap_err();
        match error {
            CreatorError::Xml { offset, .. } => assert!(offset > 0),
            other => panic!("expected an xml error, got {other:?}"),
        }
    }
}
