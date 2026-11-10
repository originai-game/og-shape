//! A JSON reader for imported glTF: the whole document into a [`Value`].
//! Numbers are parsed by `core`'s correctly rounded `f64` parser, so the
//! same text gives the same bits on every host.

use std::fmt;

/// A parsed JSON value. Objects keep their keys in file order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

/// Where a document stopped being JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParseError {
    pub at: usize,
    pub what: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.what, self.at)
    }
}

static NULL: Value = Value::Null;

impl Value {
    /// The member `key` of an object, or null.
    pub(crate) fn get(&self, key: &str) -> &Value {
        match self {
            Value::Obj(fields) => fields
                .iter()
                .find(|(k, _)| k == key)
                .map_or(&NULL, |(_, v)| v),
            _ => &NULL,
        }
    }

    /// The `i`th item of an array, or null.
    pub(crate) fn at(&self, i: usize) -> &Value {
        match self {
            Value::Arr(items) => items.get(i).unwrap_or(&NULL),
            _ => &NULL,
        }
    }

    pub(crate) fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub(crate) fn num(&self) -> Option<f64> {
        match self {
            Value::Num(v) => Some(*v),
            _ => None,
        }
    }

    /// A non-negative integer that fits in `usize`.
    pub(crate) fn index(&self) -> Option<usize> {
        match self {
            Value::Num(v) if *v >= 0.0 && v.fract() == 0.0 && *v < 9.0e15 => Some(*v as usize),
            _ => None,
        }
    }

    pub(crate) fn str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The items of an array; empty for anything else.
    pub(crate) fn items(&self) -> &[Value] {
        match self {
            Value::Arr(items) => items,
            _ => &[],
        }
    }

    /// The members of an object, in file order; empty for anything else.
    pub(crate) fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Obj(fields) => fields,
            _ => &[],
        }
    }

    /// An array of numbers, if this is one.
    pub(crate) fn nums(&self) -> Option<Vec<f64>> {
        match self {
            Value::Arr(items) => items.iter().map(Value::num).collect(),
            _ => None,
        }
    }
}

/// Parses a whole document. Nesting is limited to 128 levels.
pub(crate) fn parse(text: &[u8]) -> Result<Value, ParseError> {
    let mut p = Parser { s: text, i: 0 };
    // A UTF-8 byte order mark is allowed before the document.
    if p.s.starts_with(&[0xEF, 0xBB, 0xBF]) {
        p.i = 3;
    }
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.err("trailing characters"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, what: &'static str) -> ParseError {
        ParseError { at: self.i, what }
    }

    fn ws(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.s.get(self.i) {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth > 128 {
            return Err(self.err("nesting too deep"));
        }
        match self.s.get(self.i) {
            None => Err(self.err("unexpected end")),
            Some(b'{') => {
                self.i += 1;
                let mut fields = Vec::new();
                self.ws();
                if self.eat(b"}") {
                    return Ok(Value::Obj(fields));
                }
                loop {
                    self.ws();
                    if self.s.get(self.i) != Some(&b'"') {
                        return Err(self.err("expected a key"));
                    }
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(b":") {
                        return Err(self.err("expected ':'"));
                    }
                    self.ws();
                    let v = self.value(depth + 1)?;
                    fields.push((k, v));
                    self.ws();
                    if self.eat(b",") {
                        continue;
                    }
                    if self.eat(b"}") {
                        return Ok(Value::Obj(fields));
                    }
                    return Err(self.err("expected ',' or '}'"));
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat(b"]") {
                    return Ok(Value::Arr(items));
                }
                loop {
                    self.ws();
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat(b",") {
                        continue;
                    }
                    if self.eat(b"]") {
                        return Ok(Value::Arr(items));
                    }
                    return Err(self.err("expected ',' or ']'"));
                }
            }
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') if self.eat(b"true") => Ok(Value::Bool(true)),
            Some(b'f') if self.eat(b"false") => Ok(Value::Bool(false)),
            Some(b'n') if self.eat(b"null") => Ok(Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.i;
        while let Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') = self.s.get(self.i) {
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| self.err("number"))?;
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(Value::Num(v)),
            _ => Err(ParseError {
                at: start,
                what: "bad number",
            }),
        }
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .ok_or_else(|| self.err("short \\u escape"))?;
        let text = std::str::from_utf8(h).map_err(|_| self.err("bad \\u escape"))?;
        let v = u32::from_str_radix(text, 16).map_err(|_| self.err("bad \\u escape"))?;
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.s.get(self.i) else {
                return Err(self.err("unterminated string"));
            };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.s.get(self.i) else {
                        return Err(self.err("unterminated string"));
                    };
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) && self.eat(b"\\u") {
                                let lo = self.hex4()?;
                                0x10000 + ((hi - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF)
                            } else {
                                hi
                            };
                            char::from_u32(code).unwrap_or('\u{FFFD}')
                        }
                        _ => return Err(self.err("bad escape")),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                c => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| self.err("string is not UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_documents() {
        let v =
            parse(br#" {"a":[1,-2.5e3,true,null],"b":{"c":"x\u00e9\"\ud83d\ude00"}} "#).unwrap();
        assert_eq!(v.get("a").nums(), None);
        assert_eq!(v.get("a").at(1).num(), Some(-2500.0));
        assert_eq!(v.get("a").at(2).bool(), Some(true));
        assert_eq!(v.get("b").get("c").str(), Some("xé\"😀"));
        assert!(v.get("missing").is_null());
        assert!(parse(b"[1,]").is_err());
        assert!(parse(b"{\"a\":1} x").is_err());
    }
}
