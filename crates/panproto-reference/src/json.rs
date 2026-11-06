//! A minimal JSON reader for constraint values: the members of a JSON
//! array, each a string's contents or another value's text as written.
//!
//! The grammar is RFC 8259's: `value = object / array / number / string /
//! true / false / null`, with insignificant whitespace (space, tab, line
//! feed, carriage return) around tokens.

/// One member of a JSON array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Member {
    /// A string, by its decoded contents.
    Str(String),
    /// Any other value, by its text as written.
    Text(String),
}

/// A JSON value.
///
/// Object members stay in source order and are not deduplicated. Consumers
/// that assign semantics to member names can therefore reject duplicate
/// names instead of silently choosing one occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A number, kept exactly as written.
    Number(String),
    /// A decoded string.
    String(String),
    /// An array.
    Array(Vec<Self>),
    /// An object in source order.
    Object(Vec<(String, Self)>),
}

struct Reader<'a> {
    s: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn ws(&mut self) {
        while self
            .s
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.at += 1;
        }
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.s.get(self.at) == Some(&b) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn literal(&mut self, word: &[u8]) -> Option<()> {
        self.s
            .get(self.at..self.at + word.len())
            .filter(|w| *w == word)?;
        self.at += word.len();
        Some(())
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.s.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        self.at - start
    }

    fn number(&mut self) -> Option<()> {
        self.eat(b'-');
        if self.eat(b'0') {
        } else if self
            .s
            .get(self.at)
            .is_some_and(|b| (b'1'..=b'9').contains(b))
        {
            self.digits();
        } else {
            return None;
        }
        if self.eat(b'.') && self.digits() == 0 {
            return None;
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if self.digits() == 0 {
                return None;
            }
        }
        Some(())
    }

    fn hex4(&mut self) -> Option<u32> {
        let text = std::str::from_utf8(self.s.get(self.at..self.at + 4)?).ok()?;
        let v = u32::from_str_radix(text, 16).ok()?;
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        self.at += 4;
        Some(v)
    }

    fn string(&mut self) -> Option<String> {
        if !self.eat(b'"') {
            return None;
        }
        let mut out = String::new();
        loop {
            let b = *self.s.get(self.at)?;
            match b {
                b'"' => {
                    self.at += 1;
                    return Some(out);
                }
                b'\\' => {
                    self.at += 1;
                    let e = *self.s.get(self.at)?;
                    self.at += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let c = if (0xD800..0xDC00).contains(&hi) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return None;
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return None;
                                }
                                char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00))?
                            } else {
                                char::from_u32(hi)?
                            };
                            out.push(c);
                        }
                        _ => return None,
                    }
                }
                0x00..=0x1F => return None,
                _ => {
                    let rest = std::str::from_utf8(&self.s[self.at..]).ok()?;
                    let c = rest.chars().next()?;
                    out.push(c);
                    self.at += c.len_utf8();
                }
            }
        }
    }

    fn value(&mut self) -> Option<Value> {
        self.ws();
        match self.s.get(self.at)? {
            b'"' => self.string().map(Value::String),
            b'[' => {
                self.at += 1;
                self.ws();
                if self.eat(b']') {
                    return Some(Value::Array(Vec::new()));
                }
                let mut values = Vec::new();
                loop {
                    values.push(self.value()?);
                    self.ws();
                    if self.eat(b']') {
                        return Some(Value::Array(values));
                    }
                    if !self.eat(b',') {
                        return None;
                    }
                }
            }
            b'{' => {
                self.at += 1;
                self.ws();
                if self.eat(b'}') {
                    return Some(Value::Object(Vec::new()));
                }
                let mut members = Vec::new();
                loop {
                    self.ws();
                    let key = self.string()?;
                    self.ws();
                    if !self.eat(b':') {
                        return None;
                    }
                    let value = self.value()?;
                    members.push((key, value));
                    self.ws();
                    if self.eat(b'}') {
                        return Some(Value::Object(members));
                    }
                    if !self.eat(b',') {
                        return None;
                    }
                }
            }
            b't' => self.literal(b"true").map(|()| Value::Bool(true)),
            b'f' => self.literal(b"false").map(|()| Value::Bool(false)),
            b'n' => self.literal(b"null").map(|()| Value::Null),
            _ => {
                let start = self.at;
                self.number()?;
                Some(Value::Number(
                    std::str::from_utf8(&self.s[start..self.at])
                        .ok()?
                        .to_owned(),
                ))
            }
        }
    }
}

/// Read exactly one JSON value, allowing RFC 8259 whitespace around it.
#[must_use]
pub fn parse(text: &str) -> Option<Value> {
    let mut reader = Reader {
        s: text.as_bytes(),
        at: 0,
    };
    let value = reader.value()?;
    reader.ws();
    (reader.at == reader.s.len()).then_some(value)
}

/// The members of `text` read as one JSON array, or `None` when `text` is
/// not exactly one JSON array (whitespace around it allowed).
#[must_use]
pub fn array_members(text: &str) -> Option<Vec<Member>> {
    let mut r = Reader {
        s: text.as_bytes(),
        at: 0,
    };
    r.ws();
    if !r.eat(b'[') {
        return None;
    }
    let mut out = Vec::new();
    r.ws();
    if !r.eat(b']') {
        loop {
            r.ws();
            let start = r.at;
            let member = if r.s.get(r.at) == Some(&b'"') {
                Member::Str(r.string()?)
            } else {
                let _ = r.value()?;
                Member::Text(std::str::from_utf8(&r.s[start..r.at]).ok()?.to_owned())
            };
            out.push(member);
            r.ws();
            if r.eat(b']') {
                break;
            }
            if !r.eat(b',') {
                return None;
            }
        }
    }
    r.ws();
    (r.at == r.s.len()).then_some(out)
}
