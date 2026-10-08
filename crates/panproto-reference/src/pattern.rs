//! A naive backtracking matcher for the `Pattern` constraint semantics.
//!
//! The semantics compiles a pattern with the `regex` crate's syntax and
//! asks whether it matches anywhere in a string (an unanchored search).
//! This module parses that syntax and decides a match by exhaustive
//! backtracking, which is exact for the question "is there a match": the
//! answer does not depend on the order in which alternatives or repetition
//! counts are tried.
//!
//! Supported: literals and escaped metacharacters; `\n \t \r \f \v \a`,
//! `\x7F`, `\x{10FFFF}`, `\u{..}`; `.` (any character but a line feed);
//! `\s` and `\S` (the Unicode `White_Space` property, which
//! [`char::is_whitespace`] is); `^`, `$`, `\A` and `\z` (start and end of
//! the text); groups `(..)`, `(?:..)`, `(?P<n>..)` and `(?<n>..)`;
//! alternation; the repetitions `* + ? {n} {n,} {n,m}`, greedy or lazy;
//! classes `[..]` with negation, ranges, the escapes above and the ASCII
//! classes `[:alpha:]` and its kin.
//!
//! Refused as [`PatternError::Invalid`] (the `regex` crate rejects them):
//! look-around, back-references, an unknown escape, an unbalanced group or
//! class, a repetition with nothing to repeat or a malformed count.
//!
//! Refused as [`PatternError::Unsupported`] (their meaning needs a Unicode
//! property or case-folding table, which this crate does not carry): `\d \D
//! \w \W \b \B \p \P`, inline flags, nested classes and class set
//! operations.

/// Why a pattern does not compile here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternError {
    /// The `regex` syntax rejects the pattern.
    Invalid(String),
    /// The pattern uses a construct whose meaning needs a Unicode table.
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ClassItem {
    Range(char, char),
    Space { negated: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Empty,
    Char(char),
    Any,
    Class {
        items: Vec<ClassItem>,
        negated: bool,
    },
    Start,
    End,
    Concat(Vec<Self>),
    Alt(Vec<Self>),
    Repeat {
        node: Box<Self>,
        min: u32,
        max: Option<u32>,
    },
}

/// A compiled pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    root: Node,
}

struct Parser<'a> {
    chars: &'a [char],
    at: usize,
}

const ASCII_CLASSES: [(&str, &[(char, char)]); 14] = [
    ("alnum", &[('0', '9'), ('A', 'Z'), ('a', 'z')]),
    ("alpha", &[('A', 'Z'), ('a', 'z')]),
    ("ascii", &[('\u{0}', '\u{7F}')]),
    ("blank", &[('\t', '\t'), (' ', ' ')]),
    ("cntrl", &[('\u{0}', '\u{1F}'), ('\u{7F}', '\u{7F}')]),
    ("digit", &[('0', '9')]),
    ("graph", &[('!', '~')]),
    ("lower", &[('a', 'z')]),
    ("print", &[(' ', '~')]),
    ("punct", &[('!', '/'), (':', '@'), ('[', '`'), ('{', '~')]),
    ("space", &[('\t', '\r'), (' ', ' ')]),
    ("upper", &[('A', 'Z')]),
    ("word", &[('0', '9'), ('A', 'Z'), ('a', 'z'), ('_', '_')]),
    ("xdigit", &[('0', '9'), ('A', 'F'), ('a', 'f')]),
];

fn invalid<T>(why: &str) -> Result<T, PatternError> {
    Err(PatternError::Invalid(why.to_owned()))
}

fn unsupported<T>(why: &str) -> Result<T, PatternError> {
    Err(PatternError::Unsupported(why.to_owned()))
}

/// One escaped character, outside or inside a class.
enum Escaped {
    Char(char),
    Space { negated: bool },
    Start,
    End,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn alternation(&mut self) -> Result<Node, PatternError> {
        let mut branches = vec![self.concat()?];
        while self.eat('|') {
            branches.push(self.concat()?);
        }
        Ok(if branches.len() == 1 {
            branches.remove(0)
        } else {
            Node::Alt(branches)
        })
    }

    fn concat(&mut self) -> Result<Node, PatternError> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let atom = self.atom()?;
            items.push(self.repetition(atom)?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.remove(0),
            _ => Node::Concat(items),
        })
    }

    fn number(&mut self) -> Option<u32> {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        let text: String = self.chars[start..self.at].iter().collect();
        text.parse().ok()
    }

    fn repetition(&mut self, mut atom: Node) -> Result<Node, PatternError> {
        loop {
            let (min, max) = match self.peek() {
                Some('*') => {
                    self.at += 1;
                    (0, None)
                }
                Some('+') => {
                    self.at += 1;
                    (1, None)
                }
                Some('?') => {
                    self.at += 1;
                    (0, Some(1))
                }
                Some('{') => {
                    self.at += 1;
                    let Some(lo) = self.number() else {
                        return invalid("a repetition count is not a decimal number");
                    };
                    let hi = if self.eat(',') {
                        if self.peek() == Some('}') {
                            None
                        } else {
                            let Some(hi) = self.number() else {
                                return invalid("a repetition bound is not a decimal number");
                            };
                            Some(hi)
                        }
                    } else {
                        Some(lo)
                    };
                    if !self.eat('}') || hi.is_some_and(|h| h < lo) {
                        return invalid("a malformed repetition count");
                    }
                    (lo, hi)
                }
                _ => return Ok(atom),
            };
            // A lazy marker changes which match is found first, never
            // whether one exists.
            self.eat('?');
            atom = Node::Repeat {
                node: Box::new(atom),
                min,
                max,
            };
        }
    }

    fn atom(&mut self) -> Result<Node, PatternError> {
        let Some(c) = self.peek() else {
            return invalid("an unexpected end of the pattern");
        };
        self.at += 1;
        match c {
            '.' => Ok(Node::Any),
            '^' => Ok(Node::Start),
            '$' => Ok(Node::End),
            '(' => self.group(),
            '[' => self.class(),
            '*' | '+' | '?' | '{' => invalid("a repetition with nothing to repeat"),
            '\\' => match self.escape(false)? {
                Escaped::Char(ch) => Ok(Node::Char(ch)),
                Escaped::Space { negated } => Ok(Node::Class {
                    items: vec![ClassItem::Space { negated }],
                    negated: false,
                }),
                Escaped::Start => Ok(Node::Start),
                Escaped::End => Ok(Node::End),
            },
            other => Ok(Node::Char(other)),
        }
    }

    fn group(&mut self) -> Result<Node, PatternError> {
        if self.eat('?') {
            if self.eat(':') {
            } else if self.eat('P') || self.peek() == Some('<') {
                if !self.eat('<') {
                    return invalid("a malformed group name");
                }
                if self.peek() == Some('=') || self.peek() == Some('!') {
                    return invalid("look-behind is not supported by the syntax");
                }
                let start = self.at;
                while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    self.at += 1;
                }
                if self.at == start || !self.eat('>') {
                    return invalid("a malformed group name");
                }
            } else if self.peek() == Some('=') || self.peek() == Some('!') {
                return invalid("look-ahead is not supported by the syntax");
            } else {
                return unsupported("inline flags");
            }
        }
        let inner = self.alternation()?;
        if !self.eat(')') {
            return invalid("an unclosed group");
        }
        Ok(inner)
    }

    fn hex(&mut self, digits: Option<usize>) -> Result<char, PatternError> {
        let braced = digits.is_none() || self.peek() == Some('{');
        let text: String = if braced {
            if !self.eat('{') {
                return invalid("a malformed hexadecimal escape");
            }
            let start = self.at;
            while self.peek().is_some_and(|c| c != '}') {
                self.at += 1;
            }
            let t = self.chars[start..self.at].iter().collect();
            if !self.eat('}') {
                return invalid("a malformed hexadecimal escape");
            }
            t
        } else {
            let n = digits.unwrap_or(2);
            if self.at + n > self.chars.len() {
                return invalid("a malformed hexadecimal escape");
            }
            let t = self.chars[self.at..self.at + n].iter().collect();
            self.at += n;
            t
        };
        u32::from_str_radix(&text, 16)
            .ok()
            .filter(|_| !text.is_empty() && text.len() <= 8)
            .and_then(char::from_u32)
            .map_or_else(|| invalid("a hexadecimal escape names no character"), Ok)
    }

    fn escape(&mut self, in_class: bool) -> Result<Escaped, PatternError> {
        let Some(c) = self.peek() else {
            return invalid("an escape at the end of the pattern");
        };
        self.at += 1;
        Ok(match c {
            'n' => Escaped::Char('\n'),
            't' => Escaped::Char('\t'),
            'r' => Escaped::Char('\r'),
            'f' => Escaped::Char('\u{C}'),
            'v' => Escaped::Char('\u{B}'),
            'a' => Escaped::Char('\u{7}'),
            'x' => Escaped::Char(self.hex(Some(2))?),
            'u' => Escaped::Char(self.hex(Some(4))?),
            'U' => Escaped::Char(self.hex(Some(8))?),
            's' => Escaped::Space { negated: false },
            'S' => Escaped::Space { negated: true },
            'A' if !in_class => Escaped::Start,
            'z' if !in_class => Escaped::End,
            'd' | 'D' | 'w' | 'W' | 'p' | 'P' => {
                return unsupported("a Unicode class escape");
            }
            'b' | 'B' if !in_class => return unsupported("a word boundary"),
            '0'..='9' => return invalid("back-references are not supported by the syntax"),
            c if c.is_ascii_punctuation() || c == ' ' => Escaped::Char(c),
            _ => return invalid("an unrecognized escape"),
        })
    }

    fn class_char(&mut self) -> Result<Option<ClassItem>, PatternError> {
        let Some(c) = self.peek() else {
            return invalid("an unclosed class");
        };
        if c == '[' {
            return unsupported("a nested class");
        }
        self.at += 1;
        let lo = if c == '\\' {
            match self.escape(true)? {
                Escaped::Char(ch) => ch,
                Escaped::Space { negated } => return Ok(Some(ClassItem::Space { negated })),
                Escaped::Start | Escaped::End => return invalid("an anchor inside a class"),
            }
        } else {
            c
        };
        if (lo == '&' && self.peek() == Some('&'))
            || (lo == '-' && self.peek() == Some('-'))
            || (lo == '~' && self.peek() == Some('~'))
        {
            return unsupported("a class set operation");
        }
        if self.peek() == Some('-') && self.chars.get(self.at + 1).is_some_and(|n| *n != ']') {
            self.at += 1;
            let Some(h) = self.peek() else {
                return invalid("an unclosed class");
            };
            self.at += 1;
            let hi = if h == '\\' {
                match self.escape(true)? {
                    Escaped::Char(ch) => ch,
                    _ => return invalid("a range ending in a class"),
                }
            } else {
                h
            };
            if hi < lo {
                return invalid("a range whose end precedes its start");
            }
            return Ok(Some(ClassItem::Range(lo, hi)));
        }
        Ok(Some(ClassItem::Range(lo, lo)))
    }

    fn class(&mut self) -> Result<Node, PatternError> {
        let negated = self.eat('^');
        let mut items = Vec::new();
        let mut first = true;
        loop {
            match self.peek() {
                None => return invalid("an unclosed class"),
                Some(']') if !first => {
                    self.at += 1;
                    break;
                }
                Some(']') => {
                    self.at += 1;
                    items.push(ClassItem::Range(']', ']'));
                }
                Some('[') if self.chars.get(self.at + 1) == Some(&':') => {
                    let rest: String = self.chars[self.at + 2..].iter().collect();
                    let Some(end) = rest.find(":]") else {
                        return unsupported("a nested class");
                    };
                    let name = &rest[..end];
                    if name.starts_with('^') {
                        return unsupported("a negated ASCII class");
                    }
                    let Some((_, ranges)) = ASCII_CLASSES.iter().find(|(n, _)| *n == name) else {
                        return invalid("an unknown ASCII class");
                    };
                    items.extend(ranges.iter().map(|(a, b)| ClassItem::Range(*a, *b)));
                    self.at += 2 + end + 2;
                }
                Some(_) => {
                    if let Some(item) = self.class_char()? {
                        items.push(item);
                    }
                }
            }
            first = false;
        }
        Ok(Node::Class { items, negated })
    }
}

/// Compile a pattern.
///
/// # Errors
///
/// [`PatternError::Invalid`] for a pattern the syntax rejects, and
/// [`PatternError::Unsupported`] for one using a construct that needs a
/// Unicode table.
pub fn compile(pattern: &str) -> Result<Pattern, PatternError> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut p = Parser {
        chars: &chars,
        at: 0,
    };
    let root = p.alternation()?;
    if p.at != chars.len() {
        return invalid("an unopened group");
    }
    Ok(Pattern { root })
}

fn class_has(items: &[ClassItem], c: char) -> bool {
    items.iter().any(|i| match i {
        ClassItem::Range(a, b) => *a <= c && c <= *b,
        ClassItem::Space { negated } => c.is_whitespace() != *negated,
    })
}

/// Every position at which `node` can end when it starts at `at`.
fn ends(node: &Node, text: &[char], at: usize) -> Vec<usize> {
    match node {
        Node::Empty => vec![at],
        Node::Char(c) => {
            if text.get(at) == Some(c) {
                vec![at + 1]
            } else {
                vec![]
            }
        }
        Node::Any => match text.get(at) {
            Some(c) if *c != '\n' => vec![at + 1],
            _ => vec![],
        },
        Node::Class { items, negated } => match text.get(at) {
            Some(c) if class_has(items, *c) != *negated => vec![at + 1],
            _ => vec![],
        },
        Node::Start => {
            if at == 0 {
                vec![at]
            } else {
                vec![]
            }
        }
        Node::End => {
            if at == text.len() {
                vec![at]
            } else {
                vec![]
            }
        }
        Node::Concat(items) => {
            let mut frontier = vec![at];
            for item in items {
                let mut next: Vec<usize> =
                    frontier.iter().flat_map(|p| ends(item, text, *p)).collect();
                next.sort_unstable();
                next.dedup();
                frontier = next;
            }
            frontier
        }
        Node::Alt(branches) => {
            let mut out: Vec<usize> = branches.iter().flat_map(|b| ends(b, text, at)).collect();
            out.sort_unstable();
            out.dedup();
            out
        }
        Node::Repeat { node, min, max } => {
            let step = |from: &[usize]| -> Vec<usize> {
                let mut next: Vec<usize> = from.iter().flat_map(|p| ends(node, text, *p)).collect();
                next.sort_unstable();
                next.dedup();
                next
            };
            // The positions after exactly `min` repetitions.
            let mut reached = vec![at];
            for _ in 0..*min {
                reached = step(&reached);
                if reached.is_empty() {
                    return Vec::new();
                }
            }
            // Then every further repetition up to `max`; a position already
            // reached at a count of at least `min` adds nothing new.
            let mut seen: std::collections::BTreeSet<usize> = reached.iter().copied().collect();
            let mut k = *min;
            while max.is_none_or(|m| k < m) && !reached.is_empty() {
                reached = step(&reached);
                reached.retain(|p| seen.insert(*p));
                k += 1;
            }
            seen.into_iter().collect()
        }
    }
}

impl Pattern {
    /// Whether the pattern matches anywhere in `text`.
    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        (0..=chars.len()).any(|at| !ends(&self.root, &chars, at).is_empty())
    }
}
