//! CSS Selector AST → XPath 1.0 expression compiler.
//!
//! Translates the parsed [`SelectorList`] into an XPath expression string
//! that can be evaluated by libxml2's XPath engine.

use super::parser::{
    AttrOp, AttributeSelector, Combinator, CompoundSelector, PseudoClass, Selector, SelectorList,
};

/// Escape a string for safe embedding in an XPath string literal.
/// Uses single-quote delimiters with concat() for embedded single quotes.
fn xpath_string(s: &str) -> String {
    if !s.contains('\'') {
        return format!("'{}'", s);
    }
    // If string contains single quotes, use concat() with alternating delimiters
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut use_single = true;
    for c in s.chars() {
        if c == '\'' {
            if !current.is_empty() {
                parts.push(format!("'{}'", current));
                current.clear();
            }
            parts.push("\"\'\"".to_string());
            use_single = true;
        } else if c == '"' && use_single {
            parts.push(format!("'\"'"));
            use_single = false;
            current.clear();
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

/// Compile a [`CompoundSelector`] into an XPath node-test + predicates.
///
/// Returns (tag_test, predicates) where:
/// - `tag_test` is the XPath node test (e.g., `a`, `*`, etc.)
/// - `predicates` is a list of predicate expressions (without the `[]`)
fn compile_compound(cs: &CompoundSelector) -> (String, Vec<String>) {
    let tag = cs.tag.as_deref().unwrap_or("*");
    let mut preds = Vec::new();

    // ID selector: [@id='foo']
    if let Some(ref id) = cs.id {
        preds.push(format!("@id={}", xpath_string(id)));
    }

    // Class selectors: [contains(concat(' ', @class, ' '), ' class ')]
    for class in &cs.classes {
        preds.push(format!(
            "contains(concat(' ', normalize-space(@class), ' '), {})",
            xpath_string(&format!(" {} ", class))
        ));
    }

    // Attribute selectors
    for attr in &cs.attrs {
        let attr_pred = compile_attr(attr);
        preds.push(attr_pred);
    }

    // Pseudo-classes
    for pc in &cs.pseudo_classes {
        if let Some(pred) = compile_pseudo_class(pc, tag) {
            preds.push(pred);
        }
    }

    (tag.to_string(), preds)
}

fn compile_attr(attr: &AttributeSelector) -> String {
    let val = xpath_string(&attr.value);

    if attr.case_insensitive {
        // Use translate() for case-insensitive comparison
        let trans = |s: &str| -> String {
            format!(
                "translate({}, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz')",
                s
            )
        };
        match attr.op {
            AttrOp::Present => format!("@{}", attr.name),
            AttrOp::Exact => format!("{} = {}", trans(&format!("@{}", attr.name)), trans(&val)),
            AttrOp::Prefix => format!(
                "starts-with({}, {})",
                trans(&format!("@{}", attr.name)),
                trans(&val)
            ),
            AttrOp::Suffix => {
                let att = format!("@{}", attr.name);
                format!(
                    "substring({0}, string-length({0}) - string-length({1}) + 1) = {1}",
                    trans(&att),
                    trans(&val)
                )
            }
            AttrOp::Substring => format!(
                "contains({}, {})",
                trans(&format!("@{}", attr.name)),
                trans(&val)
            ),
            AttrOp::Include => format!(
                "contains(concat(' ', normalize-space({}), ' '), {})",
                trans(&format!("@{}", attr.name)),
                trans(&xpath_string(&format!(" {} ", attr.value)))
            ),
            AttrOp::Dash => format!(
                "{} = {} or starts-with({}, concat({}, '-'))",
                trans(&format!("@{}", attr.name)),
                trans(&val),
                trans(&format!("@{}", attr.name)),
                trans(&val)
            ),
        }
    } else {
        match attr.op {
        AttrOp::Present => format!("@{}", attr.name),
        AttrOp::Exact => format!("@{}={}", attr.name, xpath_string(&attr.value)),
        AttrOp::Prefix => format!(
            "starts-with(@{}, {})",
            attr.name,
            xpath_string(&attr.value)
        ),
        AttrOp::Suffix => {
            let val = xpath_string(&attr.value);
            format!(
                "substring(@{}, string-length(@{}) - string-length({}) + 1) = {}",
                attr.name, attr.name, val, val
            )
        }
        AttrOp::Substring => format!(
            "contains(@{}, {})",
            attr.name,
            xpath_string(&attr.value)
        ),
        AttrOp::Include => format!(
            "contains(concat(' ', normalize-space(@{}), ' '), {})",
            attr.name,
            xpath_string(&format!(" {} ", attr.value))
        ),
        AttrOp::Dash => format!(
            "@{}={} or starts-with(@{}, concat({}, '-'))",
            attr.name,
            xpath_string(&attr.value),
            attr.name,
            xpath_string(&attr.value)
        ),
    }
    }
}

/// Compile a pseudo-class into an XPath predicate expression (sans brackets).
/// Returns `None` for pseudo-classes that can't be expressed in XPath 1.0.
fn compile_pseudo_class(pc: &PseudoClass, tag: &str) -> Option<String> {
    match pc {
        PseudoClass::FirstChild => Some("position() = 1".into()),
        PseudoClass::LastChild => Some("position() = last()".into()),
        PseudoClass::FirstOfType => Some(format!(
            "not(preceding-sibling::{})",
            tag
        )),
        PseudoClass::LastOfType => Some(format!(
            "not(following-sibling::{})",
            tag
        )),
        PseudoClass::OnlyChild => Some("count(../*) = 1".into()),
        PseudoClass::OnlyOfType => Some(format!(
            "count(../{}) = 1",
            tag
        )),
        PseudoClass::Empty => Some("not(node())".into()),
        PseudoClass::Root => Some("not(parent::*)".into()),
        PseudoClass::NthChild { a, b } => {
            let expr = nth_to_xpath(*a, *b, false);
            Some(expr)
        }
        PseudoClass::NthLastChild { a, b } => {
            let expr = nth_to_xpath(*a, *b, true);
            Some(expr)
        }
        PseudoClass::NthOfType { a, b } => {
            // Count preceding siblings of same type + 1
            let expr = if *a == 0 {
                format!("count(preceding-sibling::{}) + 1 = {}", tag, b)
            } else {
                // (count(preceding-sibling::tag) + 1 - b) mod a = 0 and (count(...)+1 >= b)
                format!(
                    "count(preceding-sibling::{0}) + 1 >= {1} and (count(preceding-sibling::{0}) + 1 - {1}) mod {2} = 0",
                    tag, b, a
                )
            };
            Some(expr)
        }
        PseudoClass::NthLastOfType { a, b } => {
            let expr = if *a == 0 {
                format!("count(following-sibling::{}) + 1 = {}", tag, b)
            } else {
                format!(
                    "count(following-sibling::{0}) + 1 >= {1} and (count(following-sibling::{0}) + 1 - {1}) mod {2} = 0",
                    tag, b, a
                )
            };
            Some(expr)
        }
        PseudoClass::Not(inner) => {
            let (inner_tag, inner_preds) = compile_compound(inner);
            // Build inner expression: inner_tag[pred1][pred2]...
            let inner_expr = build_step(&inner_tag, &inner_preds);
            Some(format!("not({})", inner_expr))
        }
        PseudoClass::Lang(lang) => {
            Some(format!(
                "ancestor-or-self::*[@lang][1]/@lang = {} or starts-with(ancestor-or-self::*[@lang][1]/@lang, concat({}, '-'))",
                xpath_string(lang),
                xpath_string(lang)
            ))
        }
        // Dynamic pseudo-classes: :link, :visited — no-op in static HTML
        PseudoClass::Link | PseudoClass::Visited => None,
        // Form pseudo-classes
        PseudoClass::Enabled => Some("not(@disabled)".into()),
        PseudoClass::Disabled => Some("@disabled".into()),
        PseudoClass::Checked => Some("(@checked or @selected)".into()),
        PseudoClass::Indeterminate => Some("@indeterminate".into()),
        PseudoClass::Required => Some("@required".into()),
        PseudoClass::Optional => Some("not(@required)".into()),
        PseudoClass::ReadOnly => Some("@readonly".into()),
        PseudoClass::ReadWrite => Some("not(@readonly)".into()),
        // Validity pseudo-classes (approximate in static HTML)
        PseudoClass::Valid => Some("not(@aria-invalid)".into()),
        PseudoClass::Invalid => Some("@aria-invalid".into()),
        PseudoClass::InRange => None,  // no static HTML equivalent
        PseudoClass::OutOfRange => None,
        // :target — element whose id matches the URL fragment
        PseudoClass::Target => None,  // needs document URL context
        // :is() / :matches() — translate to union
        PseudoClass::Is(_) => {
            // :is() is handled at the selector level, not the compound level
            None
        }
        // :has() — not supported in XPath 1.0 (requires bottom-up traversal)
        PseudoClass::Has(_) => None,
        // Unknown — skip
        PseudoClass::Unknown(_) => None,
    }
}

/// Build the XPath `an+b` logic for `:nth-child` and `:nth-last-child`.
///
/// In XPath, `position()` in a predicate refers to the position of the node
/// within the context node list. For `:nth-child(an+b)`, we want to match
/// elements where the 1-based index among all siblings satisfies `an+b`.
fn nth_to_xpath(a: i32, b: i32, from_end: bool) -> String {
    let count_expr = if from_end {
        "count(following-sibling::*) + 1"
    } else {
        "count(preceding-sibling::*) + 1"
    };

    if a == 0 {
        format!("{} = {}", count_expr, b)
    } else {
        format!(
            "{} >= {} and ({} - {}) mod {} = 0",
            count_expr, b, count_expr, b, a.abs()
        )
    }
}

/// Build an XPath step: `tag[pred1][pred2]...`
fn build_step(tag: &str, preds: &[String]) -> String {
    if preds.is_empty() {
        if tag == "*" {
            "node()".into()
        } else {
            tag.into()
        }
    } else {
        let pred_str = preds
            .iter()
            .map(|p| format!("[{}]", p))
            .collect::<Vec<_>>()
            .join("");
        format!("{}{}", tag, pred_str)
    }
}

/// Compile a single [`Selector`] chain into an XPath expression.
fn compile_selector(sel: &Selector) -> String {
    let parts = &sel.parts;
    if parts.is_empty() {
        return String::new();
    }

    let mut steps = Vec::new();
    let mut is_first = true;

    for (combinator, cs) in parts {
        let (tag, preds) = compile_compound(cs);

        // Handle :is() pseudo-class — it rewrites the entire selector
        // Check if any pseudo-class is an :is()
        let mut is_rewrite = false;
        for pc in &cs.pseudo_classes {
            if let PseudoClass::Is(chains) = pc {
                is_rewrite = true;
                // Compile each chain in the :is() as alternatives
                let mut alt_parts = Vec::new();
                for chain in chains {
                    if chain.is_empty() {
                        continue;
                    }
                    // Build this alternative as a compound selector chain
                    let (alt_tag, alt_preds) = compile_compound(&chain[0]);
                    let mut alt_steps = vec![build_step(&alt_tag, &alt_preds)];
                    // Additional compound selectors in the chain
                    for extra in &chain[1..] {
                        let (extra_tag, extra_preds) = compile_compound(extra);
                        alt_steps.push(build_step(&extra_tag, &extra_preds));
                    }
                    alt_parts.push(alt_steps.join("//"));
                }
                if !alt_parts.is_empty() {
                    let union = alt_parts.join(" | ");
                    steps.push(format!("({})", union));
                }
                break;
            }
        }

        if !is_rewrite {
            let step = build_step(&tag, &preds);
            steps.push(step);
        }

        // Determine the axis separator for the *next* step
        if !is_first {
            let _axis = match combinator {
                Combinator::None | Combinator::Descendant => "//",
                Combinator::Child => "/",
                Combinator::Adjacent => "/following-sibling::*[1]/self::",
                Combinator::Sibling => "/following-sibling::",
            };
            // The axis applies to how we REACHED this step from the previous one.
            // We need to inject it before the current step.
            // Actually, the steps vector is built sequentially. Let me restructure.
        }
        is_first = false;
    }

    // Build the full path
    let mut xpath = String::new();

    for (i, (combinator, _)) in parts.iter().enumerate() {
        if i == 0 {
            xpath.push_str(&steps[i]);
        } else {
            let sep = match combinator {
                Combinator::None => "//",
                Combinator::Descendant => "//",
                Combinator::Child => "/",
                Combinator::Adjacent => "/following-sibling::*[1]/self::",
                Combinator::Sibling => "/following-sibling::",
            };
            xpath.push_str(sep);
            xpath.push_str(&steps[i]);
        }
    }

    format!("//{}", xpath)
}

/// Compile a [`SelectorList`] into an XPath 1.0 expression.
///
/// Multiple selectors (comma-separated) are unioned with `|`.
///
/// # Arguments
///
/// * `list` — The parsed selector list AST.
///
/// # Returns
///
/// An XPath expression string ready for evaluation by libxml2.
pub fn compile_xpath(list: &SelectorList) -> String {
    if list.selectors.is_empty() {
        return String::new();
    }

    if list.selectors.len() == 1 {
        return compile_selector(&list.selectors[0]);
    }

    let parts: Vec<String> = list.selectors.iter().map(compile_selector).collect();
    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse_selector_list;
    use super::*;

    fn xpath(css: &str) -> String {
        let list = parse_selector_list(css).expect("parse failed");
        compile_xpath(&list)
    }

    #[test]
    fn test_bare_tag() {
        assert_eq!(xpath("a"), "//a");
    }

    #[test]
    fn test_universal() {
        assert_eq!(xpath("*"), "//node()");
    }

    #[test]
    fn test_id() {
        assert_eq!(xpath("#foo"), "//*[@id='foo']");
    }

    #[test]
    fn test_class() {
        assert_eq!(
            xpath(".bar"),
            "//*[contains(concat(' ', normalize-space(@class), ' '), ' bar ')]"
        );
    }

    #[test]
    fn test_tag_with_id_and_class() {
        let result = xpath("div#main.content");
        assert!(result.contains("//div"));
        assert!(result.contains("@id='main'"));
        assert!(result.contains("' content '"));
        // Order of predicates is: id first, then classes, then attrs, then pseudo-classes
        assert!(result.find("@id").unwrap() < result.find("contains").unwrap());
    }

    #[test]
    fn test_descendant() {
        assert_eq!(xpath("div a"), "//div//a");
    }

    #[test]
    fn test_child() {
        assert_eq!(xpath("div > a"), "//div/a");
    }

    #[test]
    fn test_adjacent() {
        assert_eq!(
            xpath("h1 + p"),
            "//h1/following-sibling::*[1]/self::p"
        );
    }

    #[test]
    fn test_sibling() {
        assert_eq!(xpath("h1 ~ p"), "//h1/following-sibling::p");
    }

    #[test]
    fn test_attr_present() {
        assert_eq!(xpath("a[href]"), "//a[@href]");
    }

    #[test]
    fn test_attr_exact() {
        assert_eq!(xpath("a[href=\"/foo\"]"), "//a[@href='/foo']");
    }

    #[test]
    fn test_attr_prefix() {
        assert_eq!(
            xpath("a[href^=\"https\"]"),
            "//a[starts-with(@href, 'https')]"
        );
    }

    #[test]
    fn test_attr_suffix() {
        let result = xpath("a[href$=\".pdf\"]");
        assert!(result.contains("substring(@href,"));
        assert!(result.contains(") = '.pdf'"));
    }

    #[test]
    fn test_attr_substring() {
        assert_eq!(
            xpath("a[href*=\"example\"]"),
            "//a[contains(@href, 'example')]"
        );
    }

    #[test]
    fn test_pseudo_first_child() {
        assert_eq!(
            xpath("li:first-child"),
            "//li[position() = 1]"
        );
    }

    #[test]
    fn test_pseudo_last_child() {
        assert_eq!(
            xpath("li:last-child"),
            "//li[position() = last()]"
        );
    }

    #[test]
    fn test_pseudo_only_child() {
        assert_eq!(
            xpath("li:only-child"),
            "//li[count(../*) = 1]"
        );
    }

    #[test]
    fn test_pseudo_not() {
        let result = xpath("a:not(.external)");
        assert!(result.contains("//a[not("));
        assert!(result.contains("contains"));
        assert!(result.contains(" external "));
    }

    #[test]
    fn test_nth_child_odd() {
        let result = xpath("li:nth-child(odd)");
        assert!(result.contains("count(preceding-sibling::*) + 1"));
        assert!(result.contains("mod 2"));
    }

    #[test]
    fn test_nth_child_formula() {
        let result = xpath("li:nth-child(2n+1)");
        assert!(result.contains("count(preceding-sibling::*) + 1"));
        assert!(result.contains("mod 2 = 0"));
    }

    #[test]
    fn test_complex() {
        let result = xpath("div.content > a[href^=\"https\"]:first-child");
        assert!(result.starts_with("//div"));
        assert!(result.contains("contains"));
        assert!(result.contains("/a"));
        assert!(result.contains("starts-with(@href, 'https')"));
        assert!(result.contains("position() = 1"));
    }

    #[test]
    fn test_grouping() {
        let result = xpath("a, b, c");
        assert_eq!(result, "//a | //b | //c");
    }

    #[test]
    fn test_string_with_single_quote() {
        let result = xpath("a[title=\"it's\"]");
        assert!(result.contains("concat("));
        assert!(result.contains("\"'\""));
    }
}
