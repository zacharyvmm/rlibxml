//! CSS selector engine for rlibxml.
//!
//! Parses CSS selector strings and compiles them into XPath 1.0 expressions
//! for evaluation by libxml2's XPath engine.
//!
//! ## Supported selectors
//!
//! | Syntax | Example | Notes |
//! |--------|---------|-------|
//! | Tag name | `a`, `div` | Also universal `*` |
//! | ID | `#my-id` | |
//! | Class | `.my-class` | Multiple: `.a.b` |
//! | Descendant | `main a` | Whitespace combinator |
//! | Child | `main > a` | Direct child only |
//! | Adjacent sibling | `h1 + p` | Immediately following |
//! | General sibling | `h1 ~ p` | All following |
//! | Attribute present | `a[href]` | |
//! | Attribute exact | `a[href="/url"]` | |
//! | Attribute prefix | `a[href^="https"]` | |
//! | Attribute suffix | `a[href$=".pdf"]` | |
//! | Attribute substring | `a[href*="example"]` | |
//! | Attribute word | `a[class~="foo"]` | Whitespace-separated |
//! | Attribute dash | `a[hreflang\|="en"]` | Hyphen-separated prefix |
//! | `:first-child` | `li:first-child` | |
//! | `:last-child` | `li:last-child` | |
//! | `:first-of-type` | `li:first-of-type` | |
//! | `:last-of-type` | `li:last-of-type` | |
//! | `:only-child` | `li:only-child` | |
//! | `:only-of-type` | `li:only-of-type` | |
//! | `:empty` | `div:empty` | No child nodes |
//! | `:root` | `:root` | Document root |
//! | `:nth-child(an+b)` | `li:nth-child(2n+1)` | odd/even/an+b |
//! | `:nth-last-child(an+b)` | `li:nth-last-child(2n)` | |
//! | `:nth-of-type(an+b)` | `p:nth-of-type(3)` | |
//! | `:not(sel)` | `a:not(.external)` | Negation |
//! | `:lang(en)` | `p:lang(en)` | |
//! | Grouping | `a, b, c` | Comma-separated union |
//!
//! ## Limitations (XPath 1.0)
//!
//! - `:has()` — not supported (requires bottom-up traversal)
//! - `:is()` / `:matches()` — limited support (compound-only within :is())
//! - `:link`, `:visited` — no-op (static HTML)
//! - Case-insensitive attribute matching — not supported
//! - `:nth-last-of-type` — approximate support
//!
//! ## Usage
//!
//! ```rust,ignore
//! use rlibxml::css::{CssSelector, ParseError};
//!
//! let sel = CssSelector::compile("div.content > a[href]").unwrap();
//! let xpath: &str = sel.as_xpath();
//! // xpath = "//div[contains(concat(' ', normalize-space(@class), ' '), ' content ')]/a[@href]"
//! ```

mod parser;
mod xpath;

use std::fmt;

pub use parser::{ParseError, SelectorList};

/// A compiled CSS selector, backed by its XPath 1.0 expression.
///
/// Created via [`CssSelector::compile`]. The resulting XPath expression
/// can be passed directly to [`HtmlDocument::xpath`](crate::HtmlDocument::xpath).
#[derive(Debug, Clone)]
pub struct CssSelector {
    /// Original CSS selector string
    css: String,
    /// Compiled XPath 1.0 expression
    xpath: String,
}

impl CssSelector {
    /// Parse and compile a CSS selector string into an XPath expression.
    ///
    /// Returns an error if the CSS selector is malformed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let sel = CssSelector::compile("div#main > a.external[href^=\"https\"]:first-child").unwrap();
    /// let doc = HtmlDocument::new(html).unwrap();
    /// let selection = doc.xpath(sel.as_xpath());
    /// ```
    pub fn compile(css: &str) -> Result<Self, ParseError> {
        // Fast-path for common selectors — avoids parser overhead
        if let Some(xpath) = fast_path_compile(css) {
            return Ok(Self {
                css: css.to_string(),
                xpath,
            });
        }

        let list = parser::parse_selector_list(css)?;
        let xpath = xpath::compile_xpath(&list);
        Ok(Self {
            css: css.to_string(),
            xpath,
        })
    }

    /// The original CSS selector string.
    pub fn css(&self) -> &str {
        &self.css
    }

    /// The compiled XPath 1.0 expression.
    pub fn as_xpath(&self) -> &str {
        &self.xpath
    }
}

impl fmt::Display for CssSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.xpath)
    }
}

/// Fast-path for common CSS selectors that don't need the full parser.
///
/// Returns `Some(xpath)` if the selector can be compiled without the parser,
/// or `None` to fall through to normal compilation.
fn fast_path_compile(css: &str) -> Option<String> {
    let s = css.trim();
    if s.is_empty() {
        return None;
    }

    // Bare tag: "div", "a", "span", "custom-element", etc.
    if is_bare_tag(s) {
        return Some(format!("//{}", s));
    }

    // Bare ID: "#my-id"
    if let Some(id) = s.strip_prefix('#') {
        if is_bare_tag(id) && !id.is_empty() {
            return Some(format!("//*[@id='{}']", escape_xpath_str(id)));
        }
    }

    // Bare class: ".my-class"
    if let Some(class) = s.strip_prefix('.') {
        if is_bare_tag(class) && !class.is_empty() {
            return Some(format!(
                "//*[contains(concat(' ', normalize-space(@class), ' '), ' {} ')]",
                escape_xpath_str(class)
            ));
        }
    }

    // Tag with ID: "div#my-id" or "a#link"
    if let Some(hash_pos) = s.find('#') {
        let tag = &s[..hash_pos];
        let id = &s[hash_pos + 1..];
        if is_bare_tag(tag) && is_bare_tag(id) && !tag.is_empty() && !id.is_empty() {
            return Some(format!("//{}[@id='{}']", tag, escape_xpath_str(id)));
        }
    }

    // Tag with class: "div.my-class"
    if let Some(dot_pos) = s.find('.') {
        let tag = &s[..dot_pos];
        let class = &s[dot_pos + 1..];
        if is_bare_tag(tag) && is_bare_tag(class) && !tag.is_empty() && !class.is_empty() {
            return Some(format!(
                "//{}[contains(concat(' ', normalize-space(@class), ' '), ' {} ')]",
                tag,
                escape_xpath_str(class)
            ));
        }
    }

    None
}

/// Escape a string for safe embedding in an XPath string literal (single-quoted).
fn escape_xpath_str(s: &str) -> String {
    if !s.contains('\'') {
        return s.to_string();
    }
    // Use concat() with alternating delimiters for strings containing single quotes
    let mut parts = Vec::new();
    let mut current = String::new();
    for c in s.chars() {
        if c == '\'' {
            if !current.is_empty() {
                parts.push(format!("'{}'", current));
                current.clear();
            }
            parts.push("\"'\"".to_string());
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(format!("'{}'", current));
    }
    if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        format!("concat({})", parts.join(", "))
    }
}

/// Returns true if `input` looks like a bare identifier (tag name, id, class).
fn is_bare_tag(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }
    input
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bare_tag_fastpath() {
        let sel = CssSelector::compile("a").unwrap();
        assert_eq!(sel.as_xpath(), "//a");
    }

    #[test]
    fn test_complex_selector() {
        let sel = CssSelector::compile("div#main.content > a[href^=\"https\"]").unwrap();
        let xpath = sel.as_xpath();
        assert!(xpath.starts_with("//div"));
        assert!(xpath.contains("@id='main'"));
        assert!(xpath.contains("content"));
        assert!(xpath.contains("/a"));
        assert!(xpath.contains("starts-with(@href, 'https')"));
    }

    #[test]
    fn test_invalid_selector() {
        assert!(CssSelector::compile("").is_err());
        assert!(CssSelector::compile("[=]").is_err());
    }

    #[test]
    fn test_grouping() {
        let sel = CssSelector::compile("a, b, c").unwrap();
        assert_eq!(sel.as_xpath(), "//a | //b | //c");
    }
}
