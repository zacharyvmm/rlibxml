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
    pub fn select(&self, css: &str) -> XPathSelection<'_> {
        let sel = CssSelector::compile(css)
            .unwrap_or_else(|e| panic!("invalid CSS selector '{}': {}", css, e));
        self.xpath(sel.as_xpath())
    }

    /// Select elements using a pre-compiled [`CssSelector`].
    ///
    /// This avoids the compilation overhead when running the same selector
    /// against multiple documents.
    pub fn select_compiled(&self, sel: &CssSelector) -> XPathSelection<'_> {
        self.xpath(sel.as_xpath())
    }

    pub fn xpath<'a>(&'a self, expr: &str) -> XPathSelection<'a> {
        xpath_eval(self.doc, expr)
    }
}

impl Drop for HtmlDocument {
    fn drop(&mut self) {
        unsafe { ffi::xmlFreeDoc(self.doc) };
    }
}

/// Serialization helpers.
impl HtmlDocument {
    /// Serialize the document as an HTML string.
    pub fn to_html(&self) -> Option<String> {
        self.root().map(|root| root.inner_html())
    }
}

/// Serialization helpers.
impl XmlDocument {
    /// Serialize the document as an XML string.
    pub fn to_string(&self) -> Option<String> {
        self.root().map(|root| root.inner_html())
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

    pub fn xpath<'a>(&'a self, expr: &str) -> XPathSelection<'a> {
        xpath_eval(self.doc, expr)
    }
}

impl Drop for XmlDocument {
    fn drop(&mut self) {
        unsafe { ffi::xmlFreeDoc(self.doc) };
    }
}

/// Shared XPath evaluation — used by both [`HtmlDocument`] and [`XmlDocument`].
fn xpath_eval<'a>(doc: *mut ffi::xmlDoc, expr: &str) -> XPathSelection<'a> {
    let ctx = unsafe { ffi::xmlXPathNewContext(doc) };
    assert!(!ctx.is_null());

    let c_expr = CString::new(expr).expect("XPath contains NUL");
    let obj = unsafe { ffi::xmlXPathEvalExpression(c_expr.as_ptr() as *const _, ctx) };

    XPathSelection {
        ctx,
        obj,
        doc,
        _marker: std::marker::PhantomData,
    }
}

pub struct XPathSelection<'a> {
    ctx: *mut ffi::xmlXPathContext,
    obj: *mut ffi::xmlXPathObject,
    doc: *mut ffi::xmlDoc,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> XPathSelection<'a> {
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

impl Drop for XPathSelection<'_> {
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
    /// The tag name of this element (e.g., `"div"`, `"a"`).
    ///
    /// Returns an empty string for non-element nodes or if the name is null.
    pub fn name(&self) -> &str {
        unsafe {
            let raw = (*self.ptr).name;
            if raw.is_null() {
                return "";
            }
            std::ffi::CStr::from_ptr(raw as *const _)
                .to_str()
                .unwrap_or("")
        }
    }

    /// The libxml2 node type (see [`node_type`] constants).
    pub fn node_type(&self) -> u32 {
        unsafe { (*self.ptr).type_ as u32 }
    }

    /// The parent of this node, if any.
    pub fn parent(&self) -> Option<Node<'a>> {
        unsafe {
            let p = (*self.ptr).parent;
            if p.is_null() || p == std::ptr::null_mut() {
                None
            } else {
                Some(Node {
                    ptr: p,
                    doc: self.doc,
                    _marker: std::marker::PhantomData,
                })
            }
        }
    }

    /// Iterate over the children of this node.
    pub fn children(&self) -> ChildrenIter<'a> {
        ChildrenIter {
            current: unsafe { (*self.ptr).children },
            doc: self.doc,
            _marker: std::marker::PhantomData,
        }
    }

    /// The next sibling (the node immediately after this one under the same parent).
    pub fn next_sibling(&self) -> Option<Node<'a>> {
        unsafe {
            let n = (*self.ptr).next;
            if n.is_null() {
                None
            } else {
                Some(Node {
                    ptr: n,
                    doc: self.doc,
                    _marker: std::marker::PhantomData,
                })
            }
        }
    }

    /// The previous sibling (the node immediately before this one under the same parent).
    pub fn prev_sibling(&self) -> Option<Node<'a>> {
        unsafe {
            let n = (*self.ptr).prev;
            if n.is_null() {
                None
            } else {
                Some(Node {
                    ptr: n,
                    doc: self.doc,
                    _marker: std::marker::PhantomData,
                })
            }
        }
    }

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

    /// Set an attribute value on this element.
    ///
    /// If the attribute already exists, its value is replaced.
    /// The returned `Option` contains the previous value if the attribute
    /// already existed (always `None` in a new set, but may differ from
    /// libxml2 behavior — check libxml2 docs).
    pub fn set_attribute(&self, name: &str, value: &str) {
        let c_name = CString::new(name).expect("attribute name contains NUL");
        let c_value = CString::new(value).expect("attribute value contains NUL");
        unsafe {
            ffi::xmlSetProp(
                self.ptr,
                c_name.as_ptr() as *const _,
                c_value.as_ptr() as *const _,
            );
        }
    }

    /// Remove an attribute from this element.
    ///
    /// Returns `true` if the attribute existed and was removed.
    pub fn remove_attribute(&self, name: &str) -> bool {
        let c_name = CString::new(name).expect("attribute name contains NUL");
        // xmlUnsetProp returns 0 on success, -1 if the attribute didn't exist
        unsafe { ffi::xmlUnsetProp(self.ptr, c_name.as_ptr() as *const _) == 0 }
    }

    /// Set the text content of this node, replacing any existing children.
    pub fn set_text_content(&self, text: &str) {
        let c_text = CString::new(text).expect("text contains NUL");
        unsafe {
            ffi::xmlNodeSetContent(self.ptr, c_text.as_ptr() as *const _);
        }
    }

    /// Append a child node to this element.
    ///
    /// # Safety
    ///
    /// The child node must have been created from the same document as this node.
    /// After appending, the child is owned by the document and will be freed
    /// when the document is dropped.
    pub fn append_child(&self, child: &Node) {
        unsafe {
            ffi::xmlAddChild(self.ptr, child.ptr);
        }
    }

    /// Append a text node with the given content to this element.
    pub fn append_text(&self, text: &str) {
        let c_text = CString::new(text).expect("text contains NUL");
        unsafe {
            let text_node = ffi::xmlNewText(c_text.as_ptr() as *const _);
            ffi::xmlAddChild(self.ptr, text_node);
        }
    }

    /// Remove this node from the tree without freeing it.
    ///
    /// After calling this, the node is detached from the document and
    /// **you are responsible for freeing it** with [`Node::free`] or
    /// re-attaching it to another parent. Failing to do either will leak memory.
    ///
    /// Use [`Node::remove_and_free`] for the common case where you want to
    /// both detach and free.
    pub fn unlink(&self) {
        unsafe {
            ffi::xmlUnlinkNode(self.ptr);
        }
    }
}

/// Document-level node creation methods.
impl XmlDocument {
    /// Create a new element node.
    ///
    /// The caller must either:
    /// 1. Append this node to the document tree (transfers ownership), or
    /// 2. Call [`Node::free`] to release the memory.
    pub fn create_element(&self, name: &str) -> Node<'_> {
        let c_name = CString::new(name).expect("element name contains NUL");
        let ptr = unsafe {
            ffi::xmlNewNode(std::ptr::null_mut(), c_name.as_ptr() as *const _)
        };
        Node {
            ptr,
            doc: self.doc,
            _marker: std::marker::PhantomData,
        }
    }

    /// Create a new text node.
    ///
    /// The caller must either:
    /// 1. Append this node to the document tree, or
    /// 2. Call [`Node::free`] to release the memory.
    pub fn create_text_node(&self, text: &str) -> Node<'_> {
        let c_text = CString::new(text).expect("text contains NUL");
        let ptr = unsafe {
            ffi::xmlNewText(c_text.as_ptr() as *const _)
        };
        Node {
            ptr,
            doc: self.doc,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'a> Node<'a> {
    /// Free this node.
    ///
    /// Only call this on nodes that are **not** currently attached to a document
    /// tree (i.e., nodes created with [`XmlDocument::create_element`] that were
    /// never appended, or nodes that have been [`Node::unlink`]ed).
    ///
    /// # Safety
    ///
    /// Calling `free` on a node still attached to the document tree will cause
    /// a double-free when the document is dropped.
    pub unsafe fn free(&self) {
        unsafe { ffi::xmlFreeNode(self.ptr) };
    }

    /// Remove this node from the tree and free it.
    ///
    /// After this call, the node and any references to it are invalid.
    pub fn remove(&self) {
        unsafe {
            ffi::xmlUnlinkNode(self.ptr);
            ffi::xmlFreeNode(self.ptr);
        }
    }
}

pub struct NodeIter<'a> {
    sel: &'a XPathSelection<'a>,
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

/// An iterator over the children of a libxml2 node.
///
/// Walks the `xmlNode::children` → `xmlNode::next` linked list.
pub struct ChildrenIter<'a> {
    current: *mut ffi::xmlNode,
    doc: *mut ffi::xmlDoc,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> Iterator for ChildrenIter<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }
        let node = self.current;
        // Advance to next sibling before returning current
        self.current = unsafe { (*self.current).next };
        Some(Node {
            ptr: node,
            doc: self.doc,
            _marker: std::marker::PhantomData,
        })
    }
}

/// libxml2 node type constants.
///
/// Use with [`Node::node_type`].
pub mod node_type {
    /// Element node (e.g., `<div>`, `<p>`).
    pub const ELEMENT: u32 = 1;
    /// Attribute node.
    pub const ATTRIBUTE: u32 = 2;
    /// Text node.
    pub const TEXT: u32 = 3;
    /// CDATA section node.
    pub const CDATA: u32 = 4;
    /// Entity reference node.
    pub const ENTITY_REF: u32 = 5;
    /// Entity declaration node.
    pub const ENTITY: u32 = 6;
    /// Processing instruction node.
    pub const PI: u32 = 7;
    /// Comment node.
    pub const COMMENT: u32 = 8;
    /// Document node.
    pub const DOCUMENT: u32 = 9;
    /// Document type node.
    pub const DOCUMENT_TYPE: u32 = 10;
    /// Document fragment node.
    pub const DOCUMENT_FRAG: u32 = 11;
    /// Notation node.
    pub const NOTATION: u32 = 12;
    /// HTML document node (libxml2-specific).
    pub const HTML_DOCUMENT: u32 = 13;
    /// DTD node.
    pub const DTD: u32 = 14;
    /// Element declaration node.
    pub const ELEMENT_DECL: u32 = 15;
    /// Attribute declaration node.
    pub const ATTRIBUTE_DECL: u32 = 16;
    /// Entity declaration node.
    pub const ENTITY_DECL: u32 = 17;
    /// Namespace declaration node.
    pub const NAMESPACE_DECL: u32 = 18;
    /// XInclude start node.
    pub const XINCLUDE_START: u32 = 19;
    /// XInclude end node.
    pub const XINCLUDE_END: u32 = 20;
}

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

    #[test]
    fn test_node_name_and_type() {
        let html = r#"<html><body><div id="main"><p>Hello</p></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("div");
        let div = sel.iter().next().unwrap();
        assert_eq!(div.name(), "div");
        assert_eq!(div.node_type(), node_type::ELEMENT);
    }

    #[test]
    fn test_node_children_iteration() {
        let html = r#"<html><body><ul><li>A</li><li>B</li><li>C</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("ul");
        let ul = sel.iter().next().unwrap();
        let items: Vec<String> = ul
            .children()
            .filter(|n| n.node_type() == node_type::ELEMENT)
            .filter_map(|n| n.text_content())
            .collect();
        assert_eq!(items, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_node_parent() {
        let html = r#"<html><body><div><p>Hello</p></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("p");
        let p = sel.iter().next().unwrap();
        let parent = p.parent().unwrap();
        assert_eq!(parent.name(), "div");
    }

    #[test]
    fn test_node_siblings() {
        let html = r#"<html><body><ul><li>A</li><li>B</li><li>C</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("li");
        let mut iter = sel.iter();
        let first = iter.next().unwrap();
        let second = iter.next().unwrap();
        let third = iter.next().unwrap();

        // next_sibling
        assert_eq!(first.next_sibling().unwrap().text_content().as_deref(), Some("B"));
        // prev_sibling
        assert_eq!(second.prev_sibling().unwrap().text_content().as_deref(), Some("A"));
        // last has no next
        assert!(third.next_sibling().is_none());
    }

    #[test]
    fn test_dom_create_and_append() {
        let xml = "<root/>";
        let doc = XmlDocument::from_string(xml).unwrap();
        let root = doc.root().unwrap();

        let child = doc.create_element("item");
        root.append_child(&child);

        // Verify via XPath
        let sel = doc.xpath("//item");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_dom_set_and_remove_attribute() {
        let xml = "<root><item/></root>";
        let doc = XmlDocument::from_string(xml).unwrap();
        let sel = doc.xpath("//item");
        let item = sel.iter().next().unwrap();

        // Set attribute
        item.set_attribute("id", "42");
        assert_eq!(item.get_attribute("id").as_deref(), Some("42"));

        // Update attribute
        item.set_attribute("id", "99");
        assert_eq!(item.get_attribute("id").as_deref(), Some("99"));

        // Remove attribute
        assert!(item.remove_attribute("id"));
        assert_eq!(item.get_attribute("id"), None);
    }

    #[test]
    fn test_dom_set_text_content() {
        let xml = "<root><item>old</item></root>";
        let doc = XmlDocument::from_string(xml).unwrap();
        let sel = doc.xpath("//item");
        let item = sel.iter().next().unwrap();

        item.set_text_content("new text");
        assert_eq!(item.text_content().as_deref(), Some("new text"));
    }

    #[test]
    fn test_dom_append_text() {
        let xml = "<root/>";
        let doc = XmlDocument::from_string(xml).unwrap();
        let root = doc.root().unwrap();

        root.append_text("hello world");
        assert_eq!(root.text_content().as_deref(), Some("hello world"));
    }

    #[test]
    fn test_has_pseudo_class() {
        let html = r#"<html><body><div><a href="/1">Link</a></div><div>No link</div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        // div:has(a) — divs that contain an <a> descendant
        let sel = doc.select("div:has(a)");
        assert_eq!(sel.len(), 1);
        let node = sel.iter().next().unwrap();
        assert_eq!(node.get_attribute("href"), None); // the div itself has no href
        // But it contains the <a>
        let a_sel = doc.select("div:has(a) a");
        assert_eq!(a_sel.len(), 1);
        assert_eq!(a_sel.iter().next().unwrap().get_attribute("href").as_deref(), Some("/1"));
    }

    // ── More CSS selector tests ───────────────────────────────────────────

    #[test]
    fn test_select_adjacent_sibling() {
        let html = r#"<html><body><h1>Title</h1><p>First</p><p>Second</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("h1 + p");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.iter().next().unwrap().text_content().as_deref(), Some("First"));
    }

    #[test]
    fn test_select_general_sibling() {
        let html = r#"<html><body><h1>Title</h1><p>A</p><p>B</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("h1 ~ p");
        assert_eq!(sel.len(), 2);
    }

    #[test]
    fn test_select_only_child() {
        let html = r#"<html><body><ul><li>Only</li></ul><ul><li>A</li><li>B</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("li:only-child");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.iter().next().unwrap().text_content().as_deref(), Some("Only"));
    }

    #[test]
    fn test_select_empty() {
        let html = r#"<html><body><div></div><div>Not empty</div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("div:empty");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_select_root() {
        let html = r#"<html><body><p>Text</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select(":root");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.iter().next().unwrap().name(), "html");
    }

    #[test]
    fn test_select_first_of_type() {
        let html = r#"<html><body><div><p>A</p><span>X</span><p>B</p></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("p:first-of-type");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.iter().next().unwrap().text_content().as_deref(), Some("A"));
    }

    #[test]
    fn test_select_last_of_type() {
        let html = r#"<html><body><div><p>A</p><span>X</span><p>B</p></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("p:last-of-type");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.iter().next().unwrap().text_content().as_deref(), Some("B"));
    }

    #[test]
    fn test_select_only_of_type() {
        let html = r#"<html><body><div><p>A</p><span>X</span></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("span:only-of-type");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_select_nth_child_odd_even() {
        let html = r#"<html><body><ul><li>A</li><li>B</li><li>C</li><li>D</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        // :nth-child(odd) — 1-based: A, C
        let odd = doc.select("li:nth-child(odd)");
        assert_eq!(odd.len(), 2);
        // :nth-child(even) — B, D
        let even = doc.select("li:nth-child(even)");
        assert_eq!(even.len(), 2);
    }

    #[test]
    fn test_select_nth_child_formula() {
        let html = r#"<html><body><ul><li>A</li><li>B</li><li>C</li><li>D</li><li>E</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        // 2n+1 → 1, 3, 5 → A, C, E
        let sel = doc.select("li:nth-child(2n+1)");
        assert_eq!(sel.len(), 3);
        let texts: Vec<_> = sel.iter().filter_map(|n| n.text_content()).collect();
        assert_eq!(texts, vec!["A", "C", "E"]);
    }

    #[test]
    fn test_select_nth_last_child() {
        let html = r#"<html><body><ul><li>A</li><li>B</li><li>C</li></ul></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        // :nth-last-child(1) — last element = C
        let sel = doc.select("li:nth-last-child(1)");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.iter().next().unwrap().text_content().as_deref(), Some("C"));
    }

    #[test]
    fn test_select_nth_of_type() {
        let html = r#"<html><body><div><span>X</span><p>A</p><p>B</p><p>C</p></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("p:nth-of-type(2)");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.iter().next().unwrap().text_content().as_deref(), Some("B"));
    }

    #[test]
    fn test_select_not_pseudo() {
        let html = r#"<html><body><a class="ext">A</a><a class="int">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a:not(.ext)");
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.iter().next().unwrap().text_content().as_deref(), Some("B"));
    }

    #[test]
    fn test_select_lang() {
        let html = r#"<html><body><p lang="en">English</p><p lang="fr">French</p><p lang="en-US">US English</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("p:lang(en)");
        assert_eq!(sel.len(), 2); // "en" and "en-US" both match
    }

    #[test]
    fn test_select_is_pseudo() {
        let html = r#"<html><body><h1>Title</h1><h2>Sub</h2><p>Text</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select(":is(h1, h2)");
        assert_eq!(sel.len(), 2);
    }

    #[test]
    fn test_select_attr_include() {
        let html = r#"<html><body><a class="foo bar">A</a><a class="foo">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a[class~=\"bar\"]");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_select_attr_dash() {
        let html = r#"<html><body><p lang="en">A</p><p lang="en-US">B</p><p lang="fr">C</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("p[lang|=\"en\"]");
        assert_eq!(sel.len(), 2);
    }

    // ── Form pseudo-class tests ───────────────────────────────────────────

    #[test]
    fn test_select_enabled_disabled() {
        let html = r#"<html><body><form><input name="a"><input name="b" disabled></form></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let enabled = doc.select("input:enabled");
        let disabled = doc.select("input:disabled");
        assert_eq!(enabled.len(), 1);
        assert_eq!(disabled.len(), 1);
        assert_eq!(enabled.iter().next().unwrap().get_attribute("name").as_deref(), Some("a"));
    }

    #[test]
    fn test_select_checked() {
        let html = r#"<html><body><form><input type="checkbox" checked><input type="checkbox"></form></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("input:checked");
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn test_select_required_optional() {
        let html = r#"<html><body><form><input required><input></form></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        assert_eq!(doc.select("input:required").len(), 1);
        assert_eq!(doc.select("input:optional").len(), 1);
    }

    #[test]
    fn test_select_readonly_readwrite() {
        let html = r#"<html><body><form><input readonly><input></form></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        assert_eq!(doc.select("input:read-only").len(), 1);
        assert_eq!(doc.select("input:read-write").len(), 1);
    }

    // ── Case-insensitive attribute test ───────────────────────────────────

    #[test]
    fn test_select_attr_case_insensitive() {
        let html = r#"<html><body><a href="HTTPS://EXAMPLE.COM">Link</a><a href="http://other.com">Other</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        // CSS4: [attr="value" i]
        let sel = doc.select(r#"a[href="https://example.com" i]"#);
        assert_eq!(sel.len(), 1);
    }

    // ── DOM manipulation edge cases ───────────────────────────────────────

    #[test]
    fn test_dom_remove_node() {
        let xml = "<root><a/><b/><c/></root>";
        let doc = XmlDocument::from_string(xml).unwrap();
        // Remove the <b/> node
        let b_sel = doc.xpath("//b");
        assert_eq!(b_sel.len(), 1);
        let b = b_sel.iter().next().unwrap();
        b.remove();

        // Verify it's gone
        let after = doc.xpath("//b");
        assert_eq!(after.len(), 0);
        // But a and c remain
        assert_eq!(doc.xpath("//a").len(), 1);
        assert_eq!(doc.xpath("//c").len(), 1);
    }

    #[test]
    fn test_dom_unlink_and_free() {
        let xml = "<root><a/></root>";
        let doc = XmlDocument::from_string(xml).unwrap();
        let a_sel = doc.xpath("//a");
        let a = a_sel.iter().next().unwrap();
        a.unlink();
        // Node is now detached; free it
        unsafe { a.free(); }
        // Document no longer contains <a/>
        assert_eq!(doc.xpath("//a").len(), 0);
    }

    #[test]
    fn test_html_serialization_roundtrip() {
        let html = r#"<html><body><p>Hello</p></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let serialized = doc.to_html().unwrap();
        assert!(serialized.contains("<p>Hello</p>"), "got: {serialized}");
    }

    #[test]
    fn test_xml_serialization_roundtrip() {
        let xml = "<root><item>Value</item></root>";
        let doc = XmlDocument::from_string(xml).unwrap();
        let serialized = doc.to_string().unwrap();
        assert!(serialized.contains("<item>Value</item>"), "got: {serialized}");
    }

    // ── XPath edge cases ──────────────────────────────────────────────────

    #[test]
    fn test_xpath_text_node() {
        let xml = "<root>hello</root>";
        let doc = XmlDocument::from_string(xml).unwrap();
        let sel = doc.xpath("//root/text()");
        assert_eq!(sel.len(), 1);
        let node = sel.iter().next().unwrap();
        assert_eq!(node.name(), "text");
        assert_eq!(node.node_type(), node_type::TEXT);
    }

    #[test]
    fn test_xpath_count() {
        let xml = "<root><a/><b/><c/></root>";
        let doc = XmlDocument::from_string(xml).unwrap();
        // count(//*) — XPath 1.0 count function
        let sel = doc.xpath("count(//*)");
        assert_eq!(sel.len(), 0); // count returns a number, not a nodeset
    }

    #[test]
    fn test_xpath_attribute_axis() {
        let xml = r#"<root><item id="42"/></root>"#;
        let doc = XmlDocument::from_string(xml).unwrap();
        let sel = doc.xpath("//item/@id");
        assert_eq!(sel.len(), 1);
    }

    // ── Error cases ───────────────────────────────────────────────────────

    #[test]
    fn test_html_null_input() {
        assert!(HtmlDocument::new("").is_some()); // empty string parses to empty doc
    }

    #[test]
    fn test_css_compile_error() {
        assert!(CssSelector::compile("[=]").is_err());
        assert!(CssSelector::compile("").is_err());
        assert!(CssSelector::compile("  ").is_err());
    }

    #[test]
    fn test_exact_size_iterator() {
        let html = r#"<html><body><a>A</a><a>B</a><a>C</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a");
        let mut iter = sel.iter();
        assert_eq!(iter.len(), 3);
        iter.next();
        assert_eq!(iter.len(), 2);
    }

    #[test]
    fn test_children_iterator_empty() {
        let html = r#"<html><body><div></div></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("div");
        let div = sel.iter().next().unwrap();
        let children: Vec<_> = div.children().collect();
        assert!(children.is_empty());
    }
}
