mod ffi;
pub mod css;

use std::ffi::CString;
use std::os::raw::c_int;

use css::CssSelector;

pub struct HtmlDocument {
    doc: *mut ffi::xmlDoc,
}

impl HtmlDocument {
    pub fn new(html: &str) -> Option<Self> {
        let doc = unsafe {
            ffi::htmlReadMemory(
                html.as_ptr() as *const _,
                html.len() as c_int,
                std::ptr::null(),
                std::ptr::null(),
                (ffi::htmlParserOption_HTML_PARSE_RECOVER
                    | ffi::htmlParserOption_HTML_PARSE_NOERROR
                    | ffi::htmlParserOption_HTML_PARSE_NOWARNING) as c_int,
            )
        };
        if doc.is_null() { None } else { Some(Self { doc }) }
    }

    pub fn as_ptr(&self) -> *mut ffi::xmlDoc {
        self.doc
    }

    pub fn root(&self) -> Option<Node<'_>> {
        unsafe {
            let root = ffi::xmlDocGetRootElement(self.doc);
            if root.is_null() { None }
            else {
                Some(Node {
                    ptr: root,
                    doc: self.doc,
                    _marker: std::marker::PhantomData,
                })
            }
        }
    }

    /// Select elements matching a CSS selector string.
    ///
    /// This compiles the CSS selector to XPath internally and evaluates it.
    /// For performance, compile once with [`CssSelector::compile`] and call
    /// [`HtmlDocument::xpath`] directly when running the same selector against
    /// multiple documents.
    ///
    /// # Panics
    ///
    /// Panics if the CSS selector is syntactically invalid.
    /// Use [`CssSelector::compile`] for fallible compilation.
    pub fn select(&self, css: &str) -> XPathSelection {
        let sel = CssSelector::compile(css)
            .unwrap_or_else(|e| panic!("invalid CSS selector '{}': {}", css, e));
        self.xpath(sel.as_xpath())
    }

    /// Select elements using a pre-compiled [`CssSelector`].
    ///
    /// This avoids the compilation overhead when running the same selector
    /// against multiple documents.
    pub fn select_compiled(&self, sel: &CssSelector) -> XPathSelection {
        self.xpath(sel.as_xpath())
    }

    pub fn xpath(&self, expr: &str) -> XPathSelection {
        xpath_eval(self.doc, expr)
    }
}

impl Drop for HtmlDocument {
    fn drop(&mut self) {
        unsafe { ffi::xmlFreeDoc(self.doc) };
    }
}

/// An XML document parsed by libxml2.
///
/// Like [`HtmlDocument`] but uses the XML parser (`xmlReadMemory`) instead of
/// the HTML parser, which means the document must be well-formed XML.
pub struct XmlDocument {
    doc: *mut ffi::xmlDoc,
}

impl XmlDocument {
    /// Parse an XML string.
    ///
    /// Returns `None` if libxml2 could not parse the document (e.g., it is not
    /// well-formed XML). For error details, use a custom error handler.
    pub fn from_string(xml: &str) -> Option<Self> {
        let doc = unsafe {
            ffi::xmlReadMemory(
                xml.as_ptr() as *const _,
                xml.len() as c_int,
                std::ptr::null(),   // base URL
                std::ptr::null(),   // encoding (auto-detect)
                0,                   // options
            )
        };
        if doc.is_null() { None } else { Some(Self { doc }) }
    }

    pub fn as_ptr(&self) -> *mut ffi::xmlDoc {
        self.doc
    }

    pub fn root(&self) -> Option<Node<'_>> {
        unsafe {
            let root = ffi::xmlDocGetRootElement(self.doc);
            if root.is_null() { None }
            else {
                Some(Node {
                    ptr: root,
                    doc: self.doc,
                    _marker: std::marker::PhantomData,
                })
            }
        }
    }

    /// Select elements matching a CSS selector string.
    ///
    /// # Panics
    ///
    /// Panics if the CSS selector is syntactically invalid.
    pub fn select(&self, css: &str) -> XPathSelection {
        let sel = CssSelector::compile(css)
            .unwrap_or_else(|e| panic!("invalid CSS selector '{}': {}", css, e));
        self.xpath(sel.as_xpath())
    }

    /// Select elements using a pre-compiled [`CssSelector`].
    pub fn select_compiled(&self, sel: &CssSelector) -> XPathSelection {
        self.xpath(sel.as_xpath())
    }

    pub fn xpath(&self, expr: &str) -> XPathSelection {
        xpath_eval(self.doc, expr)
    }
}

impl Drop for XmlDocument {
    fn drop(&mut self) {
        unsafe { ffi::xmlFreeDoc(self.doc) };
    }
}

/// Shared XPath evaluation — used by both [`HtmlDocument`] and [`XmlDocument`].
fn xpath_eval(doc: *mut ffi::xmlDoc, expr: &str) -> XPathSelection {
    let ctx = unsafe { ffi::xmlXPathNewContext(doc) };
    assert!(!ctx.is_null());

    let c_expr = CString::new(expr).expect("XPath contains NUL");
    let obj = unsafe { ffi::xmlXPathEvalExpression(c_expr.as_ptr() as *const _, ctx) };

    XPathSelection { ctx, obj, doc }
}

pub struct XPathSelection {
    ctx: *mut ffi::xmlXPathContext,
    obj: *mut ffi::xmlXPathObject,
    doc: *mut ffi::xmlDoc,
}

impl XPathSelection {
    pub fn len(&self) -> usize {
        self.nodeset_len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> NodeIter<'_> {
        NodeIter { sel: self, index: 0 }
    }

    #[inline]
    fn nodeset_len(&self) -> usize {
        if self.obj.is_null() {
            return 0;
        }
        let ns = unsafe { (*self.obj).nodesetval };
        if ns.is_null() { 0 } else { unsafe { (*ns).nodeNr as usize } }
    }

    #[inline]
    fn nodeset_get(&self, i: usize) -> *mut ffi::xmlNode {
        debug_assert!(i < self.nodeset_len());
        unsafe {
            let ns = (*self.obj).nodesetval;
            *(*ns).nodeTab.add(i)
        }
    }
}

impl Drop for XPathSelection {
    fn drop(&mut self) {
        if !self.obj.is_null() {
            unsafe { ffi::xmlXPathFreeObject(self.obj) };
        }
        unsafe { ffi::xmlXPathFreeContext(self.ctx) };
    }
}

pub struct Node<'a> {
    ptr: *mut ffi::xmlNode,
    doc: *mut ffi::xmlDoc,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> Node<'a> {
    pub fn text_content(&self) -> Option<String> {
        unsafe {
            let raw = ffi::xmlNodeGetContent(self.ptr);
            if raw.is_null() {
                return None;
            }
            let s = std::ffi::CStr::from_ptr(raw as *const _)
                .to_string_lossy()
                .into_owned();
            ffi::xmlFree.expect("libxml2 xmlFree is unavailable")(raw as *mut _);
            Some(s)
        }
    }

    pub fn get_attribute(&self, name: &str) -> Option<String> {
        let c_name = CString::new(name).ok()?;
        unsafe {
            let raw = ffi::xmlGetProp(self.ptr, c_name.as_ptr() as *const _);
            if raw.is_null() {
                return None;
            }
            let s = std::ffi::CStr::from_ptr(raw as *const _)
                .to_string_lossy()
                .into_owned();
            ffi::xmlFree.expect("libxml2 xmlFree is unavailable")(raw as *mut _);
            Some(s)
        }
    }

    pub fn inner_html(&self) -> String {
        unsafe {
            let buf = ffi::xmlBufferCreate();
            assert!(!buf.is_null());

            let mut child = (*self.ptr).children;
            while !child.is_null() {
                ffi::xmlNodeDump(buf, self.doc, child, 0, 0);
                child = (*child).next;
            }

            let ptr = ffi::xmlBufferContent(buf);
            let len = ffi::xmlBufferLength(buf) as usize;
            let s = if ptr.is_null() || len == 0 {
                String::new()
            } else {
                let slice = std::slice::from_raw_parts(ptr, len);
                String::from_utf8_lossy(slice).into_owned()
            };
            ffi::xmlBufferFree(buf);
            s
        }
    }
}

pub struct NodeIter<'a> {
    sel: &'a XPathSelection,
    index: usize,
}

impl<'a> Iterator for NodeIter<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.sel.nodeset_len() {
            return None;
        }
        let ptr = self.sel.nodeset_get(self.index);
        self.index += 1;
        Some(Node {
            ptr,
            doc: self.sel.doc,
            _marker: std::marker::PhantomData,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.sel.nodeset_len() - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for NodeIter<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_select_bare_tag() {
        let html = r#"<html><body><a href="/1">A</a><a href="/2">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a");
        assert_eq!(sel.len(), 2);
    }

    #[test]
    fn test_select_descendant() {
        let html = r#"<html><body><div><a href="/1">A</a><p><a href="/2">B</a></p></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("div a");
        assert_eq!(sel.len(), 2);
    }

    #[test]
    fn test_select_child() {
        let html = r#"<html><body><div><a href="/1">A</a><p><a href="/2">B</a></p></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("div > a");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_select_id() {
        let html = r#"<html><body><a id="link1">A</a><a id="link2">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("#link1");
        assert_eq!(sel.len(), 1);
        let node = sel.iter().next().unwrap();
        assert_eq!(node.text_content().as_deref(), Some("A"));
    }

    #[test]
    fn test_select_class() {
        let html = r#"<html><body><a class="ext">A</a><a class="int ext">B</a><a class="int">C</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select(".ext");
        assert_eq!(sel.len(), 2);
    }

    #[test]
    fn test_attr_present() {
        let html = r#"<html><body><a>A</a><a href="/1">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a[href]");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_attr_exact() {
        let html = r#"<html><body><a href="/target">A</a><a href="/other">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a[href=\"/target\"]");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_attr_prefix() {
        let html = r#"<html><body><a href="https://a.com">A</a><a href="http://b.com">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a[href^=\"https\"]");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_attr_suffix() {
        let html = r#"<html><body><a href="/a.pdf">A</a><a href="/b.html">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a[href$=\".pdf\"]");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_attr_substring() {
        let html = r#"<html><body><a href="/foo/bar">A</a><a href="/baz">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a[href*=\"bar\"]");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_first_child() {
        let html = r#"<html><body><ul><li>A</li><li>B</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("li:first-child");
        assert_eq!(sel.len(), 1);
        let node = sel.iter().next().unwrap();
        assert_eq!(node.text_content().as_deref(), Some("A"));
    }

    #[test]
    fn test_last_child() {
        let html = r#"<html><body><ul><li>A</li><li>B</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("li:last-child");
        assert_eq!(sel.len(), 1);
        let node = sel.iter().next().unwrap();
        assert_eq!(node.text_content().as_deref(), Some("B"));
    }

    #[test]
    fn test_nth_child() {
        let html = r#"<html><body><ul><li>A</li><li>B</li><li>C</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("li:nth-child(2)");
        assert_eq!(sel.len(), 1);
        let node = sel.iter().next().unwrap();
        assert_eq!(node.text_content().as_deref(), Some("B"));
    }

    #[test]
    fn test_grouping() {
        let html = r#"<html><body><p>A</p><a href="/1">B</a><p>C</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a, p");
        assert_eq!(sel.len(), 3);
    }

    #[test]
    fn test_attributes() {
        let html = r#"<html><body><a href="/foo">Link</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a");
        let node = sel.iter().next().unwrap();
        assert_eq!(node.get_attribute("href").as_deref(), Some("/foo"));
        assert_eq!(node.get_attribute("nope"), None);
    }

    #[test]
    fn test_text_content() {
        let html = r#"<html><body><a><b>Hello</b> World</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a");
        let node = sel.iter().next().unwrap();
        assert_eq!(node.text_content().as_deref(), Some("Hello World"));
    }

    #[test]
    fn test_inner_html() {
        let html = r#"<html><body><a><b>Hello</b> World</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a");
        let node = sel.iter().next().unwrap();
        let inner = node.inner_html();
        assert!(inner.contains("<b>Hello</b>"), "got: {inner}");
        assert!(inner.contains("World"), "got: {inner}");
    }

    #[test]
    fn test_no_matches() {
        let html = r#"<html><body><p>Hi</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a");
        assert_eq!(sel.len(), 0);
        assert_eq!(sel.iter().count(), 0);
    }

    #[test]
    fn test_select_compiled_reuse() {
        let sel = CssSelector::compile("a").unwrap();
        let html1 = r#"<html><body><a href="/1">A</a><a href="/2">B</a></body></html>"#;
        let html2 = r#"<html><body><a href="/3">C</a></body></html>"#;

        let doc1 = HtmlDocument::new(html1).unwrap();
        let result1 = doc1.select_compiled(&sel);
        assert_eq!(result1.len(), 2);

        let doc2 = HtmlDocument::new(html2).unwrap();
        let result2 = doc2.select_compiled(&sel);
        assert_eq!(result2.len(), 1);
    }

    #[test]
    #[should_panic(expected = "invalid CSS selector")]
    fn test_invalid_selector_panics() {
        let html = r#"<html></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        doc.select("!!invalid!!");
    }

    #[test]
    fn test_xml_document_parse_and_select() {
        let xml = r#"<root><item id="1">A</item><item id="2">B</item></root>"#;
        let doc = XmlDocument::from_string(xml).unwrap();
        let sel = doc.select("item");
        assert_eq!(sel.len(), 2);
    }

    #[test]
    fn test_xml_document_xpath() {
        let xml = r#"<root><item id="1">A</item><item id="2">B</item></root>"#;
        let doc = XmlDocument::from_string(xml).unwrap();
        let sel = doc.xpath("//item[@id='1']");
        assert_eq!(sel.len(), 1);
        let node = sel.iter().next().unwrap();
        assert_eq!(node.text_content().as_deref(), Some("A"));
    }

    #[test]
    fn test_xml_document_root() {
        let xml = r#"<root><child/></root>"#;
        let doc = XmlDocument::from_string(xml).unwrap();
        let root = doc.root().unwrap();
        assert_eq!(root.text_content().as_deref(), Some(""));
    }

    #[test]
    fn test_xml_malformed() {
        // Not well-formed XML (missing closing tag)
        assert!(XmlDocument::from_string("<root><child>").is_none());
    }
}
