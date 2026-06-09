use std::env;
use std::path::PathBuf;
use std::process::Command;

struct Libxml {
    include_paths: Vec<PathBuf>,
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let lib = if cfg!(feature = "vendored") {
        build_vendored_libxml()
    } else {
        find_system_libxml()
    };

    let mut builder = bindgen::Builder::default()
        .header_contents(
            "wrapper.h",
            "#include <libxml/HTMLparser.h>\n\
             #include <libxml/xpath.h>\n\
             #include <libxml/tree.h>\n\
             #include <libxml/xmlmemory.h>\n",
        )
        .allowlist_function("htmlReadMemory")
        .allowlist_function("xmlReadMemory")
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
        .allowlist_var("xmlFree")
        .allowlist_type("xmlNode")
        .allowlist_type("xmlXPathObject")
        .allowlist_type("xmlNodeSet")
        .allowlist_type("xmlFreeFunc")
        .allowlist_type("htmlParserOption")
        .derive_default(false)
        .merge_extern_blocks(true)
        .layout_tests(false);

    for path in &lib.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    let bindings = builder.generate().expect("bindgen failed");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("failed to write bindings");
}

fn find_system_libxml() -> Libxml {
    let lib = pkg_config::Config::new()
        .atleast_version("2.9")
        .probe("libxml-2.0")
        .expect("libxml-2.0 not found via pkg-config; enable the `vendored` feature to build libxml2 from source");

    Libxml {
        include_paths: lib.include_paths,
    }
}

fn build_vendored_libxml() -> Libxml {
    require_tool("git", "clone the latest libxml2 sources");
    require_tool("cmake", "configure and build vendored libxml2");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let source = out.join("libxml2-src");
    let build = out.join("libxml2-build");
    let install = out.join("libxml2-install");

    if !source.exists() {
        run(Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "https://gitlab.gnome.org/GNOME/libxml2.git",
            ])
            .arg(&source));
    }

    run(Command::new("cmake")
        .arg("-S")
        .arg(&source)
        .arg("-B")
        .arg(&build)
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", install.display()))
        .args([
            "-DBUILD_SHARED_LIBS=OFF",
            "-DLIBXML2_WITH_C14N=OFF",
            "-DLIBXML2_WITH_CATALOG=OFF",
            "-DLIBXML2_WITH_DEBUG=OFF",
            "-DLIBXML2_WITH_FTP=OFF",
            "-DLIBXML2_WITH_HISTORY=OFF",
            "-DLIBXML2_WITH_HTML=ON",
            "-DLIBXML2_WITH_HTTP=OFF",
            "-DLIBXML2_WITH_ICONV=OFF",
            "-DLIBXML2_WITH_ICU=OFF",
            "-DLIBXML2_WITH_LZMA=OFF",
            "-DLIBXML2_WITH_MODULES=OFF",
            "-DLIBXML2_WITH_PROGRAMS=OFF",
            "-DLIBXML2_WITH_PYTHON=OFF",
            "-DLIBXML2_WITH_READER=ON",
            "-DLIBXML2_WITH_REGEXPS=ON",
            "-DLIBXML2_WITH_SAX1=ON",
            "-DLIBXML2_WITH_SCHEMAS=OFF",
            "-DLIBXML2_WITH_SCHEMATRON=OFF",
            "-DLIBXML2_WITH_TESTS=OFF",
            "-DLIBXML2_WITH_THREADS=OFF",
            "-DLIBXML2_WITH_TREE=ON",
            "-DLIBXML2_WITH_VALID=OFF",
            "-DLIBXML2_WITH_WRITER=ON",
            "-DLIBXML2_WITH_XINCLUDE=OFF",
            "-DLIBXML2_WITH_XPATH=ON",
            "-DLIBXML2_WITH_XPTR=OFF",
            "-DLIBXML2_WITH_ZLIB=OFF",
        ]));
    run(Command::new("cmake").arg("--build").arg(&build));
    run(Command::new("cmake").arg("--install").arg(&build));

    let lib_dir = install.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=xml2");

    Libxml {
        include_paths: vec![install.join("include").join("libxml2")],
    }
}

fn run(command: &mut Command) {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .unwrap_or_else(|err| panic!("failed to run `{program}`: {err}"));
    if !status.success() {
        panic!("`{program}` exited with {status}");
    }
}

fn require_tool(program: &str, purpose: &str) {
    let found = Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());

    if !found {
        panic!("`{program}` is required to {purpose} when the `vendored` feature is enabled");
    }
}
