mod ffi;

use std::ffi::CString;
use std::os::raw::c_int;

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

    pub fn select(&self, css: &str) -> XPathSelection {
        let xpath = format!("//{css}");
        self.xpath(&xpath)
    }

    pub fn xpath(&self, expr: &str) -> XPathSelection {
        let ctx = unsafe { ffi::xmlXPathNewContext(self.doc) };
        assert!(!ctx.is_null());

        let c_expr = CString::new(expr).expect("XPath contains NUL");
        let obj = unsafe { ffi::xmlXPathEvalExpression(c_expr.as_ptr() as *const _, ctx) };

        XPathSelection { ctx, obj, doc: self.doc }
    }
}

impl Drop for HtmlDocument {
    fn drop(&mut self) {
        unsafe { ffi::xmlFreeDoc(self.doc) };
    }
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
    fn test_parse_and_select() {
        let html = r#"<html><body><a href="/1">A</a><a href="/2">B</a></body></html>"#;
        let doc = HtmlDocument::new(html).unwrap();
        let sel = doc.select("a");
        assert_eq!(sel.len(), 2);
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
}
