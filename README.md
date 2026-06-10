# rlibxml

[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

A thin, safe Rust wrapper around [libxml2](https://gitlab.gnome.org/GNOME/libxml2) providing HTML/XML parsing, DOM navigation and manipulation, XPath evaluation, and CSS selector support — with correct `Drop` semantics and minimal overhead.

```rust
use rlibxml::HtmlDocument;

let html = r#"<html><body><div class="content"><a href="/1">Link</a></div></body></html>"#;
let doc = HtmlDocument::new(html).unwrap();

// CSS selectors
let links = doc.select("div.content > a[href]");
println!("Found {} link(s)", links.len());

// XPath
let items = doc.xpath("//a/@href");
for node in items.iter() {
    println!("href: {:?}", node.text_content());
}
```

## Features

- **HTML parsing** via `HtmlDocument` (tolerant, like a browser)
- **XML parsing** via `XmlDocument` (strict, well-formed only)
- **DOM navigation** — `parent()`, `children()`, `next_sibling()`, `prev_sibling()`, `name()`, `node_type()`
- **DOM manipulation** — create elements/text nodes, set/get/remove attributes, set text content, append/remove children
- **DOM serialization** — `to_html()` / `to_string()`
- **XPath 1.0** — full XPath evaluation with namespace support
- **CSS selectors** — 35+ selector types compiled to XPath (see below)
- **Memory safety** — all `unsafe` blocks audited, `XPathSelection` lifetime-bound to prevent use-after-free, proper `Drop` on all types
- **Vendored libxml2** — optional `vendored` feature builds libxml2 from source (no system dependency)

## Supported CSS Selectors

| Category | Selectors |
|----------|-----------|
| **Basic** | `tag`, `*` (universal), `#id`, `.class`, `.class1.class2` |
| **Combinators** | `div a` (descendant), `div > a` (child), `h1 + p` (adjacent sibling), `h1 ~ p` (general sibling) |
| **Attributes** | `[attr]`, `[attr="v"]`, `[attr^="prefix"]`, `[attr$="suffix"]`, `[attr*="substr"]`, `[attr~="word"]`, `[attr\|="dash"]`, `[attr="v" i]` (case-insensitive) |
| **Structural** | `:first-child`, `:last-child`, `:only-child`, `:first-of-type`, `:last-of-type`, `:only-of-type`, `:empty`, `:root`, `:nth-child(an+b)`, `:nth-last-child(an+b)`, `:nth-of-type(an+b)`, `:nth-last-of-type(an+b)` |
| **Form** | `:enabled`, `:disabled`, `:checked`, `:indeterminate`, `:required`, `:optional`, `:read-only`, `:read-write` |
| **Other** | `:not(sel)`, `:has(sel)`, `:is(sel, sel)`, `:lang(en)`, `:valid`, `:invalid` |
| **Grouping** | `a, b, c` (union) |

CSS selectors are compiled to XPath 1.0 at parse time. For repeated queries, pre-compile with `CssSelector::compile()` and use `select_compiled()`.

## Quick Start

Add to `Cargo.toml`:

```toml
[dependencies]
rlibxml = { version = "0.1", features = ["vendored"] }
```

Or use system libxml2 (requires `libxml2-dev`):

```toml
[dependencies]
rlibxml = "0.1"
```

### Parse and Select

```rust
use rlibxml::{HtmlDocument, XmlDocument};
use rlibxml::css::CssSelector;

// HTML (forgiving parser)
let doc = HtmlDocument::new("<html><body><p>Hello</p></body></html>").unwrap();
let paras = doc.select("p");

// XML (strict parser)
let doc = XmlDocument::from_string("<root><item id='1'>A</item></root>").unwrap();
let items = doc.xpath("//item[@id='1']");

// Pre-compiled CSS for reuse
let sel = CssSelector::compile("a.external[href^=\"https\"]").unwrap();
let links = doc.select_compiled(&sel);
```

### DOM Navigation

```rust
let doc = HtmlDocument::new("<ul><li>A</li><li>B</li></ul>").unwrap();
let ul = doc.select("ul").iter().next().unwrap();

for child in ul.children() {
    println!("{}: {:?}", child.name(), child.text_content());
}
```

### DOM Manipulation

```rust
use rlibxml::XmlDocument;

let doc = XmlDocument::from_string("<root/>").unwrap();
let root = doc.root().unwrap();

// Create and append
let item = doc.create_element("item");
item.set_attribute("id", "42");
item.set_text_content("Hello");
root.append_child(&item);

// Remove
item.remove();

println!("{}", doc.to_string().unwrap());
```

## Safety

This crate wraps libxml2's C API with thin safe abstractions. Key safety properties:

- **Drop correctness** — all libxml2 allocations are freed via `Drop` impls
- **Lifetime enforcement** — `XPathSelection<'a>` borrows the document, preventing use-after-free
- **Null checks** — all C return values are checked before dereference
- **NUL safety** — all strings passed to C are validated via `CString::new()`
- **32 `unsafe` blocks** — each audited with documented invariants

## Benchmarks

Run with `cargo bench --features vendored`:

| Operation | Time |
|-----------|------|
| CSS compile (bare tag) | ~47 ns |
| Select bare tag | ~1.0 µs |
| XPath direct | ~2.1 µs |
| CSS compile (complex) | ~3.5 µs |
| HTML parse (small doc) | ~8.5 µs |
| `:has()` selector | ~10.2 µs |

## Limitations

- **`:has()` with combinators** — child/adjacent/sibling combinators inside `:has()` are compiled as descendant by default
- **Namespace-prefixed XPath** — requires binding namespaces via `xmlXPathRegisterNs` (not yet exposed in the safe API)
- **Pseudo-elements** (`::before`, `::after`) — parsed but not selectable (they don't exist in the DOM tree)
- **Dynamic pseudo-classes** (`:hover`, `:focus`, `:active`) — no-ops in static HTML/XML

## License

MIT
