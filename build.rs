use std::env;
use std::path::PathBuf;

fn main() {
    let lib = pkg_config::Config::new()
        .atleast_version("2.9")
        .probe("libxml-2.0")
        .expect("libxml-2.0 not found via pkg-config");

    let mut builder = bindgen::Builder::default()
        .header_contents(
            "wrapper.h",
            "#include <libxml/HTMLparser.h>\n\
             #include <libxml/xpath.h>\n\
             #include <libxml/tree.h>\n",
        )
        .allowlist_function("htmlReadMemory")
        .allowlist_function("xmlFreeDoc")
        .allowlist_function("xmlDocGetRootElement")
        .allowlist_function("xmlXPathNewContext")
        .allowlist_function("xmlXPathFreeContext")
        .allowlist_function("xmlXPathEvalExpression")
        .allowlist_function("xmlXPathFreeObject")
        .allowlist_function("xmlNodeGetContent")
        .allowlist_function("xmlGetProp")
        .allowlist_function("xmlBufferCreate")
        .allowlist_function("xmlBufferFree")
        .allowlist_function("xmlBufferContent")
        .allowlist_function("xmlBufferLength")
        .allowlist_function("xmlNodeDump")
        .allowlist_function("xmlMemFree")
        .allowlist_type("xmlNode")
        .allowlist_type("xmlXPathObject")
        .allowlist_type("xmlNodeSet")
        .allowlist_type("htmlParserOption")
        .derive_default(false)
        .merge_extern_blocks(true)
        .layout_tests(false);

    for path in &lib.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    let bindings = builder.generate().expect("bindgen failed");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("bindings.rs")).expect("failed to write bindings");
}
