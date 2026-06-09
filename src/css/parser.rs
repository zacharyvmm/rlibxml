//! CSS selector parser.
//!
//! Tokenizer + recursive-descent parser producing a [`SelectorList`] AST.
//! Supports Selectors Level 3 with some Level 4 extensions.

use std::fmt;

// ── Tokenizer ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Hash(String),   // #id — value is the id name (without #)
    String_(String),
    Delim(char),
    DashMatch,    // |=
    PrefixMatch,  // ^=
    SuffixMatch,  // $=
    SubstrMatch,  // *=
    IncludeMatch, // ~=
    Whitespace,
    Eof,
}

struct Tokenizer<'a> {
    chars: std::str::Chars<'a>,
    peeked: Option<char>,
    lookahead: Vec<Token>,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        let mut chars = input.chars();
        let peeked = chars.next();
        Self { chars, peeked, lookahead: Vec::new() }
    }

    fn peek_char(&self) -> Option<char> {
        self.peeked
    }

    fn advance_char(&mut self) -> Option<char> {
        let c = self.peeked;
        self.peeked = self.chars.next();
        c
    }

    fn consume_while(&mut self, pred: fn(char) -> bool) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek_char() {
            if pred(c) {
                s.push(c);
                self.advance_char();
            } else {
                break;
            }
        }
        s
    }

    fn next_token(&mut self) -> Token {
        // Drain any pushed-back tokens first
        if let Some(tok) = self.lookahead.pop() {
            return tok;
        }

        // Skip comments /* ... */
        while self.peek_char() == Some('/') {
            self.advance_char();
            if self.peek_char() == Some('*') {
                self.advance_char();
                self.skip_comment();
            } else {
                return Token::Delim('/');
            }
        }

        match self.peek_char() {
            None => Token::Eof,
            Some(c) if c.is_whitespace() => {
                self.consume_while(|c| c.is_whitespace());
                Token::Whitespace
            }
            Some('"') | Some('\'') => {
                let quote = self.advance_char().unwrap();
                let mut s = String::new();
                loop {
                    match self.advance_char() {
                        None => break,
                        Some(c) if c == quote => break,
                        Some('\\') => {
                            if let Some(escaped) = self.advance_char() {
                                s.push(escaped);
                            }
                        }
                        Some(c) => s.push(c),
                    }
                }
                Token::String_(s)
            }
            Some('#') => {
                self.advance_char();
                let name = self.consume_ident();
                if name.is_empty() {
                    Token::Delim('#')
                } else {
                    Token::Hash(name)
                }
            }
            Some('.') => {
                self.advance_char();
                if self.peek_char().map_or(false, |c| c.is_ascii_digit()) {
                    Token::Delim('.')
                } else {
                    let name = self.consume_ident();
                    if name.is_empty() {
                        Token::Delim('.')
                    } else {
                        // Emit as Delim('.') followed by Ident — push ident back
                        self.lookahead.push(Token::Ident(name));
                        Token::Delim('.')
                    }
                }
            }
            Some(':') => {
                self.advance_char();
                Token::Delim(':')
            }
            Some('*') => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    Token::SubstrMatch
                } else {
                    Token::Delim('*')
                }
            }
            Some('~') => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    Token::IncludeMatch
                } else {
                    Token::Delim('~')
                }
            }
            Some('[') | Some(']') | Some('(') | Some(')') | Some(',')
            | Some('>') | Some('+') => {
                let c = self.advance_char().unwrap();
                Token::Delim(c)
            }
            Some('|') => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    Token::DashMatch
                } else {
                    Token::Delim('|')
                }
            }
            Some('^') => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    Token::PrefixMatch
                } else {
                    Token::Delim('^')
                }
            }
            Some('$') => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    Token::SuffixMatch
                } else {
                    Token::Delim('$')
                }
            }
            Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '-' => {
                Token::Ident(self.consume_ident())
            }
            Some(c) => {
                self.advance_char();
                Token::Delim(c)
            }
        }
    }

    fn consume_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                s.push(c);
                self.advance_char();
            } else if c == '\\' {
                self.advance_char();
                if let Some(escaped) = self.advance_char() {
                    s.push(escaped);
                }
            } else {
                break;
            }
        }
        s
    }

    fn skip_comment(&mut self) {
        loop {
            match self.advance_char() {
                None => break,
                Some('*') if self.peek_char() == Some('/') => {
                    self.advance_char();
                    break;
                }
                _ => {}
            }
        }
    }

    fn tokenize(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let is_eof = matches!(tok, Token::Eof);
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        tokens
    }
}

// ── AST types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Combinator {
    None,
    Descendant,
    Child,
    Adjacent,
    Sibling,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrOp {
    Present,
    Exact,
    Prefix,
    Suffix,
    Substring,
    Include,
    Dash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSelector {
    pub name: String,
    pub op: AttrOp,
    pub value: String,
    /// If true, the match is case-insensitive (`[attr="value" i]` in CSS4).
    pub case_insensitive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    FirstOfType,
    LastOfType,
    OnlyChild,
    OnlyOfType,
    Empty,
    Root,
    NthChild { a: i32, b: i32 },
    NthLastChild { a: i32, b: i32 },
    NthOfType { a: i32, b: i32 },
    NthLastOfType { a: i32, b: i32 },
    Not(Box<CompoundSelector>),
    Is(Vec<Vec<CompoundSelector>>),
    Has(Vec<Vec<CompoundSelector>>),
    Lang(String),
    Link,
    Visited,
    // Form pseudo-classes
    Enabled,
    Disabled,
    Checked,
    Indeterminate,
    Required,
    Optional,
    ReadOnly,
    ReadWrite,
    // Validity pseudo-classes
    Valid,
    Invalid,
    InRange,
    OutOfRange,
    // Other
    Target,
    Unknown(String),
}

/// A CSS pseudo-element (e.g., `::before`, `::after`).
///
/// Pseudo-elements select virtual portions of the document that don't
/// correspond to real DOM nodes. In an HTML/XML parsing context, they are
/// mostly informational.
#[derive(Debug, Clone, PartialEq)]
pub enum PseudoElement {
    Before,
    After,
    FirstLine,
    FirstLetter,
    Selection,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompoundSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attrs: Vec<AttributeSelector>,
    pub pseudo_classes: Vec<PseudoClass>,
}

impl CompoundSelector {
    fn new() -> Self {
        Self {
            tag: None,
            id: None,
            classes: Vec::new(),
            attrs: Vec::new(),
            pseudo_classes: Vec::new(),
        }
    }

    pub fn is_bare_tag(&self) -> bool {
        self.id.is_none()
            && self.classes.is_empty()
            && self.attrs.is_empty()
            && self.pseudo_classes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub parts: Vec<(Combinator, CompoundSelector)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectorList {
    pub selectors: Vec<Selector>,
}

// ── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CSS parse error at position {}: {}", self.position, self.message)
    }
}

impl std::error::Error for ParseError {}

// ── Parser ───────────────────────────────────────────────────────────────────

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        let idx = self.pos;
        self.pos += 1;
        self.tokens.get(idx).unwrap_or(&Token::Eof)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Token::Whitespace) {
            self.advance();
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        let tok = self.advance().clone();
        match tok {
            Token::Ident(name) => Ok(name),
            _ => Err(self.error(format!("expected identifier, got {:?}", tok))),
        }
    }

    fn error(&self, message: String) -> ParseError {
        ParseError {
            message,
            position: self.pos,
        }
    }

    fn parse_selector_list(&mut self) -> Result<SelectorList, ParseError> {
        let mut selectors = Vec::new();
        self.skip_whitespace();

        selectors.push(self.parse_selector()?);

        loop {
            self.skip_whitespace();
            if matches!(self.peek(), Token::Eof) {
                break;
            }
            if matches!(self.peek(), Token::Delim(',')) {
                self.advance();
                self.skip_whitespace();
                selectors.push(self.parse_selector()?);
            } else {
                break;
            }
        }

        Ok(SelectorList { selectors })
    }

    fn parse_selector(&mut self) -> Result<Selector, ParseError> {
        let mut parts: Vec<(Combinator, CompoundSelector)> = Vec::new();
        let mut combinator = Combinator::None;

        loop {
            self.skip_whitespace();
            if matches!(self.peek(), Token::Eof) || matches!(self.peek(), Token::Delim(',')) {
                break;
            }

            // Check for combinator
            match self.peek() {
                Token::Delim('>') => {
                    self.advance();
                    self.skip_whitespace();
                    combinator = Combinator::Child;
                    continue;
                }
                Token::Delim('+') => {
                    self.advance();
                    self.skip_whitespace();
                    combinator = Combinator::Adjacent;
                    continue;
                }
                Token::Delim('~') => {
                    self.advance();
                    self.skip_whitespace();
                    combinator = Combinator::Sibling;
                    continue;
                }
                Token::Whitespace => {
                    if !parts.is_empty() {
                        self.skip_whitespace();
                        // After whitespace, if we see Eof/comma/combinator, it wasn't a combinator
                        if matches!(
                            self.peek(),
                            Token::Eof | Token::Delim(',') | Token::Delim('>')
                                | Token::Delim('+') | Token::Delim('~')
                        ) {
                            break;
                        }
                        combinator = Combinator::Descendant;
                        continue;
                    } else {
                        self.skip_whitespace();
                        continue;
                    }
                }
                _ => {}
            }

            let compound = self.parse_compound_selector()?;
            parts.push((combinator, compound));
            combinator = Combinator::Descendant;
        }

        if parts.is_empty() {
            return Err(self.error("empty selector".into()));
        }

        Ok(Selector { parts })
    }

    fn parse_compound_selector(&mut self) -> Result<CompoundSelector, ParseError> {
        let mut cs = CompoundSelector::new();
        let mut saw_simple = false;

        loop {
            match self.peek() {
                Token::Ident(name) => {
                    let name = name.clone();
                    self.advance();
                    cs.tag = Some(name);
                    saw_simple = true;
                }
                Token::Delim('*') => {
                    self.advance();
                    saw_simple = true;
                }
                Token::Hash(id) => {
                    let id = id.clone();
                    self.advance();
                    if cs.id.is_some() {
                        return Err(self.error("duplicate id selector".into()));
                    }
                    cs.id = Some(id);
                    saw_simple = true;
                }
                Token::Delim('.') => {
                    self.advance();
                    let name = self.expect_ident()?;
                    cs.classes.push(name);
                    saw_simple = true;
                }
                Token::Delim('[') => {
                    self.advance();
                    cs.attrs.push(self.parse_attribute()?);
                    saw_simple = true;
                }
                Token::Delim(':') => {
                    self.advance();
                    cs.pseudo_classes.push(self.parse_pseudo()?);
                    saw_simple = true;
                }
                _ => break,
            }
        }

        if !saw_simple {
            return Err(self.error("expected a simple selector".into()));
        }

        Ok(cs)
    }

    fn parse_attribute(&mut self) -> Result<AttributeSelector, ParseError> {
        self.skip_whitespace();
        let name = self.expect_ident()?;
        self.skip_whitespace();

        let (op, value) = match self.peek() {
            Token::Delim(']') => {
                self.advance();
                (AttrOp::Present, String::new())
            }
            Token::Delim('=') => {
                self.advance();
                self.skip_whitespace();
                let val = self.parse_attr_value()?;
                (AttrOp::Exact, val)
            }
            Token::PrefixMatch => {
                self.advance();
                self.skip_whitespace();
                let val = self.parse_attr_value()?;
                (AttrOp::Prefix, val)
            }
            Token::SuffixMatch => {
                self.advance();
                self.skip_whitespace();
                let val = self.parse_attr_value()?;
                (AttrOp::Suffix, val)
            }
            Token::SubstrMatch => {
                self.advance();
                self.skip_whitespace();
                let val = self.parse_attr_value()?;
                (AttrOp::Substring, val)
            }
            Token::IncludeMatch => {
                self.advance();
                self.skip_whitespace();
                let val = self.parse_attr_value()?;
                (AttrOp::Include, val)
            }
            Token::DashMatch => {
                self.advance();
                self.skip_whitespace();
                let val = self.parse_attr_value()?;
                (AttrOp::Dash, val)
            }
            tok => {
                return Err(self.error(format!(
                    "expected attribute operator or ']', got {:?}", tok
                )));
            }
        };
        self.skip_whitespace();

        // CSS4 case-insensitive flag: [attr=value i]
        let case_insensitive =
            matches!(self.peek(), Token::Ident(s) if s.eq_ignore_ascii_case("i"));
        if case_insensitive {
            self.advance();
            self.skip_whitespace();
        }

        if !matches!(op, AttrOp::Present) {
            self.expect_delim(']')?;
        }

        Ok(AttributeSelector {
            name,
            op,
            value,
            case_insensitive,
        })
    }

    fn parse_attr_value(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Token::String_(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            Token::Ident(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            tok => Err(self.error(format!("expected string or identifier, got {:?}", tok))),
        }
    }

    fn expect_delim(&mut self, expected: char) -> Result<(), ParseError> {
        let tok = self.advance().clone();
        match tok {
            Token::Delim(c) if c == expected => Ok(()),
            _ => Err(self.error(format!("expected '{}', got {:?}", expected, tok))),
        }
    }

    fn parse_pseudo(&mut self) -> Result<PseudoClass, ParseError> {
        let name = match self.peek() {
            Token::Ident(n) => {
                let n = n.clone();
                self.advance();
                n
            }
            Token::Delim(':') => {
                self.advance();
                self.expect_ident()?
            }
            tok => return Err(self.error(format!("expected pseudo-class name, got {:?}", tok))),
        };

        let name_lower = name.to_lowercase();

        if matches!(self.peek(), Token::Delim('(')) {
            self.advance();
            self.skip_whitespace();

            let pc = match name_lower.as_str() {
                "not" => {
                    let cs = self.parse_compound_selector()?;
                    PseudoClass::Not(Box::new(cs))
                }
                "is" | "matches" => {
                    let list = self.parse_is_selector_list()?;
                    PseudoClass::Is(list)
                }
                "has" => {
                    let list = self.parse_is_selector_list()?;
                    PseudoClass::Has(list)
                }
                "nth-child" => {
                    let (a, b) = self.parse_nth()?;
                    PseudoClass::NthChild { a, b }
                }
                "nth-last-child" => {
                    let (a, b) = self.parse_nth()?;
                    PseudoClass::NthLastChild { a, b }
                }
                "nth-of-type" => {
                    let (a, b) = self.parse_nth()?;
                    PseudoClass::NthOfType { a, b }
                }
                "nth-last-of-type" => {
                    let (a, b) = self.parse_nth()?;
                    PseudoClass::NthLastOfType { a, b }
                }
                "lang" => {
                    let lang = self.parse_pseudo_arg()?;
                    PseudoClass::Lang(lang)
                }
                _ => {
                    let arg = self.parse_pseudo_arg()?;
                    PseudoClass::Unknown(format!("{}({})", name, arg))
                }
            };

            self.skip_whitespace();
            self.expect_delim(')')?;
            Ok(pc)
        } else {
            match name_lower.as_str() {
                "first-child" => Ok(PseudoClass::FirstChild),
                "last-child" => Ok(PseudoClass::LastChild),
                "first-of-type" => Ok(PseudoClass::FirstOfType),
                "last-of-type" => Ok(PseudoClass::LastOfType),
                "only-child" => Ok(PseudoClass::OnlyChild),
                "only-of-type" => Ok(PseudoClass::OnlyOfType),
                "empty" => Ok(PseudoClass::Empty),
                "root" => Ok(PseudoClass::Root),
                "link" => Ok(PseudoClass::Link),
                "visited" => Ok(PseudoClass::Visited),
                "enabled" => Ok(PseudoClass::Enabled),
                "disabled" => Ok(PseudoClass::Disabled),
                "checked" => Ok(PseudoClass::Checked),
                "indeterminate" => Ok(PseudoClass::Indeterminate),
                "required" => Ok(PseudoClass::Required),
                "optional" => Ok(PseudoClass::Optional),
                "read-only" => Ok(PseudoClass::ReadOnly),
                "read-write" => Ok(PseudoClass::ReadWrite),
                "valid" => Ok(PseudoClass::Valid),
                "invalid" => Ok(PseudoClass::Invalid),
                "in-range" => Ok(PseudoClass::InRange),
                "out-of-range" => Ok(PseudoClass::OutOfRange),
                "target" => Ok(PseudoClass::Target),
                _ => Ok(PseudoClass::Unknown(name)),
            }
        }
    }

    fn parse_pseudo_arg(&mut self) -> Result<String, ParseError> {
        let mut depth = 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                Token::Eof => {
                    return Err(self.error("unterminated pseudo-class argument".into()));
                }
                Token::Delim('(') => {
                    depth += 1;
                    s.push('(');
                    self.advance();
                }
                Token::Delim(')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    s.push(')');
                    self.advance();
                }
                Token::String_(val) => {
                    s.push('"');
                    s.push_str(val);
                    s.push('"');
                    self.advance();
                }
                Token::Ident(val) => {
                    s.push_str(val);
                    self.advance();
                }
                Token::Whitespace => {
                    s.push(' ');
                    self.advance();
                }
                Token::Delim(c) => {
                    s.push(*c);
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
        Ok(s)
    }

    fn parse_is_selector_list(&mut self) -> Result<Vec<Vec<CompoundSelector>>, ParseError> {
        let mut results = Vec::new();
        loop {
            self.skip_whitespace();
            if matches!(self.peek(), Token::Delim(')') | Token::Eof) {
                break;
            }

            // Parse a chain of compound selectors with combinators
            let mut chain = Vec::new();

            loop {
                if matches!(self.peek(), Token::Delim(')') | Token::Delim(',') | Token::Eof) {
                    break;
                }

                match self.peek() {
                    Token::Delim('>') => {
                        self.advance();
                        continue;
                    }
                    Token::Delim('+') => {
                        self.advance();
                        continue;
                    }
                    Token::Delim('~') => {
                        self.advance();
                        continue;
                    }
                    Token::Whitespace => {
                        if !chain.is_empty() {
                            self.skip_whitespace();
                            continue;
                        } else {
                            self.skip_whitespace();
                            continue;
                        }
                    }
                    _ => {}
                }

                let cs = self.parse_compound_selector()?;
                chain.push(cs);
            }

            if !chain.is_empty() {
                results.push(chain);
            }

            self.skip_whitespace();
            if matches!(self.peek(), Token::Delim(',')) {
                self.advance();
                continue;
            }
            break;
        }
        Ok(results)
    }

    fn parse_nth(&mut self) -> Result<(i32, i32), ParseError> {
        let arg = self.parse_pseudo_arg()?;
        let arg = arg.trim();

        if arg == "odd" {
            return Ok((2, 1));
        }
        if arg == "even" {
            return Ok((2, 0));
        }

        let lower = arg.to_lowercase();

        if let Ok(n) = lower.trim().parse::<i32>() {
            return Ok((0, n));
        }

        let mut a: i32 = 0;
        let mut b: i32 = 0;

        let sign = if lower.trim_start().starts_with('-') { -1 } else { 1 };

        let parts: Vec<&str> = lower
            .split(|c: char| c == '+' || c == ' ')
            .filter(|s| !s.is_empty())
            .collect();

        for part in parts {
            if part.contains('n') {
                let coef: &str = part
                    .trim_start_matches(|c: char| c == '+' || c == '-')
                    .trim_end_matches('n');
                let coef = coef.trim();
                if coef.is_empty() {
                    a = sign;
                } else if coef == "-" {
                    a = -1;
                } else if let Ok(val) = coef.parse::<i32>() {
                    a = val * sign;
                }
            } else if let Ok(val) = part.parse::<i32>() {
                b = val;
            }
        }

        Ok((a, b))
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

pub fn parse_selector_list(input: &str) -> Result<SelectorList, ParseError> {
    if input.trim().is_empty() {
        return Err(ParseError { message: "empty selector".into(), position: 0 });
    }
    let tokenizer = Tokenizer::new(input);
    let tokens = tokenizer.tokenize();
    let mut parser = Parser::new(tokens);
    parser.parse_selector_list()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> SelectorList {
        parse_selector_list(input).unwrap_or_else(|e| panic!("failed to parse '{}': {}", input, e))
    }

    #[test]
    fn test_bare_tag() {
        let list = parse("a");
        let sel = &list.selectors[0];
        assert_eq!(sel.parts.len(), 1);
        assert_eq!(sel.parts[0].0, Combinator::None);
        assert_eq!(sel.parts[0].1.tag, Some("a".into()));
        assert!(sel.parts[0].1.is_bare_tag());
    }

    #[test]
    fn test_id_selector() {
        let list = parse("#foo");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.id, Some("foo".into()));
    }

    #[test]
    fn test_class_selector() {
        let list = parse(".bar");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.classes, vec!["bar"]);
    }

    #[test]
    fn test_multiple_classes() {
        let list = parse(".foo.bar");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.classes, vec!["foo", "bar"]);
    }

    #[test]
    fn test_tag_with_id_and_class() {
        let list = parse("div#main.content");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.tag, Some("div".into()));
        assert_eq!(cs.id, Some("main".into()));
        assert_eq!(cs.classes, vec!["content"]);
    }

    #[test]
    fn test_descendant() {
        let list = parse("div a");
        let sel = &list.selectors[0];
        assert_eq!(sel.parts.len(), 2);
        assert_eq!(sel.parts[1].0, Combinator::Descendant);
    }

    #[test]
    fn test_child() {
        let list = parse("div > a");
        let sel = &list.selectors[0];
        assert_eq!(sel.parts[1].0, Combinator::Child);
    }

    #[test]
    fn test_adjacent() {
        let list = parse("h1 + p");
        assert_eq!(list.selectors[0].parts[1].0, Combinator::Adjacent);
    }

    #[test]
    fn test_sibling() {
        let list = parse("h1 ~ p");
        assert_eq!(list.selectors[0].parts[1].0, Combinator::Sibling);
    }

    #[test]
    fn test_attr_present() {
        let list = parse("a[href]");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.attrs[0].name, "href");
        assert_eq!(cs.attrs[0].op, AttrOp::Present);
    }

    #[test]
    fn test_attr_exact() {
        let list = parse("a[href=\"/foo\"]");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.attrs[0].op, AttrOp::Exact);
        assert_eq!(cs.attrs[0].value, "/foo");
    }

    #[test]
    fn test_attr_prefix() {
        let list = parse("a[href^=\"https\"]");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.attrs[0].op, AttrOp::Prefix);
        assert_eq!(cs.attrs[0].value, "https");
    }

    #[test]
    fn test_attr_suffix() {
        let list = parse("a[href$=\".pdf\"]");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.attrs[0].op, AttrOp::Suffix);
        assert_eq!(cs.attrs[0].value, ".pdf");
    }

    #[test]
    fn test_attr_substring() {
        let list = parse("a[href*=\"example\"]");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.attrs[0].op, AttrOp::Substring);
        assert_eq!(cs.attrs[0].value, "example");
    }

    #[test]
    fn test_pseudo_first_child() {
        let list = parse("li:first-child");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.pseudo_classes, vec![PseudoClass::FirstChild]);
    }

    #[test]
    fn test_pseudo_last_child() {
        let list = parse("li:last-child");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.pseudo_classes, vec![PseudoClass::LastChild]);
    }

    #[test]
    fn test_pseudo_not() {
        let list = parse("a:not(.external)");
        let cs = &list.selectors[0].parts[0].1;
        match &cs.pseudo_classes[0] {
            PseudoClass::Not(inner) => assert_eq!(inner.classes, vec!["external"]),
            _ => panic!("expected Not"),
        }
    }

    #[test]
    fn test_nth_child() {
        let list = parse("li:nth-child(2n+1)");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.pseudo_classes, vec![PseudoClass::NthChild { a: 2, b: 1 }]);
    }

    #[test]
    fn test_nth_child_odd() {
        let list = parse("li:nth-child(odd)");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.pseudo_classes, vec![PseudoClass::NthChild { a: 2, b: 1 }]);
    }

    #[test]
    fn test_complex() {
        let list = parse("div.content > a[href^=\"https\"][target=\"_blank\"]:first-child");
        let sel = &list.selectors[0];
        assert_eq!(sel.parts.len(), 2);
        assert_eq!(sel.parts[1].0, Combinator::Child);
        let cs = &sel.parts[1].1;
        assert_eq!(cs.tag, Some("a".into()));
        assert_eq!(cs.attrs.len(), 2);
        assert_eq!(cs.pseudo_classes.len(), 1);
    }

    #[test]
    fn test_grouping() {
        let list = parse("a, b, c");
        assert_eq!(list.selectors.len(), 3);
    }

    #[test]
    fn test_empty() {
        assert!(parse_selector_list("").is_err());
        assert!(parse_selector_list("   ").is_err());
    }

    #[test]
    fn test_universal() {
        let list = parse("*");
        let cs = &list.selectors[0].parts[0].1;
        assert_eq!(cs.tag, None);
    }
}
