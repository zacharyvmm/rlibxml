use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rlibxml::{HtmlDocument, XmlDocument};
use rlibxml::css::CssSelector;

const SMALL_HTML: &str = r#"<html><body><div class="content"><a href="/1">Link 1</a><p>Text</p><a href="/2" class="external">Link 2</a></div></body></html>"#;

const LARGE_HTML: &str = include_str!("../benches/large.html");

fn bench_parse_html(c: &mut Criterion) {
    c.bench_function("parse_html_small", |b| {
        b.iter(|| HtmlDocument::new(black_box(SMALL_HTML)))
    });
}

fn bench_select_bare_tag(c: &mut Criterion) {
    let doc = HtmlDocument::new(SMALL_HTML).unwrap();
    c.bench_function("select_bare_tag", |b| {
        b.iter(|| doc.select(black_box("a")).len())
    });
}

fn bench_select_class(c: &mut Criterion) {
    let doc = HtmlDocument::new(SMALL_HTML).unwrap();
    c.bench_function("select_class", |b| {
        b.iter(|| doc.select(black_box(".external")).len())
    });
}

fn bench_select_child_combinator(c: &mut Criterion) {
    let doc = HtmlDocument::new(SMALL_HTML).unwrap();
    c.bench_function("select_child_combinator", |b| {
        b.iter(|| doc.select(black_box("div > a")).len())
    });
}

fn bench_select_attr_prefix(c: &mut Criterion) {
    let doc = HtmlDocument::new(SMALL_HTML).unwrap();
    c.bench_function("select_attr_prefix", |b| {
        b.iter(|| doc.select(black_box("a[href^=\"/\"]")).len())
    });
}

fn bench_select_has(c: &mut Criterion) {
    let doc = HtmlDocument::new(SMALL_HTML).unwrap();
    c.bench_function("select_has", |b| {
        b.iter(|| doc.select(black_box("div:has(a.external)")).len())
    });
}

fn bench_css_compile(c: &mut Criterion) {
    c.bench_function("css_compile_simple", |b| {
        b.iter(|| CssSelector::compile(black_box("a")))
    });
    c.bench_function("css_compile_complex", |b| {
        b.iter(|| CssSelector::compile(black_box("div#main.content > a[href^=\"https\"][target=\"_blank\"]:first-child")))
    });
}

fn bench_compiled_reuse(c: &mut Criterion) {
    let sel = CssSelector::compile("a.external").unwrap();
    let doc = HtmlDocument::new(SMALL_HTML).unwrap();
    c.bench_function("select_compiled_reuse", |b| {
        b.iter(|| doc.select_compiled(black_box(&sel)).len())
    });
}

fn bench_xpath(c: &mut Criterion) {
    let doc = HtmlDocument::new(SMALL_HTML).unwrap();
    c.bench_function("xpath_direct", |b| {
        b.iter(|| doc.xpath(black_box("//a[@href]")).len())
    });
}

fn bench_xml_parse(c: &mut Criterion) {
    let xml = r#"<root><items><item id="1">A</item><item id="2">B</item><item id="3">C</item></items></root>"#;
    c.bench_function("parse_xml", |b| {
        b.iter(|| XmlDocument::from_string(black_box(xml)))
    });
}

criterion_group!(
    benches,
    bench_parse_html,
    bench_select_bare_tag,
    bench_select_class,
    bench_select_child_combinator,
    bench_select_attr_prefix,
    bench_select_has,
    bench_css_compile,
    bench_compiled_reuse,
    bench_xpath,
    bench_xml_parse,
);
criterion_main!(benches);
