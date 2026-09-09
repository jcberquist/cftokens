use std::env;
use std::path::Path;
use syntect::dumps::dump_to_uncompressed_file;
use syntect::parsing::{Scope, SyntaxSetBuilder};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest = Path::new(&manifest_dir);

    println!("cargo:rerun-if-changed=CFML");
    println!("cargo:rerun-if-changed=Packages");

    let mut builder = SyntaxSetBuilder::new();
    builder.add_plain_text_syntax();
    builder
        .add_from_folder(manifest.join("Packages"), true)
        .expect("load Packages syntaxes");
    builder
        .add_from_folder(manifest.join("CFML"), true)
        .expect("load CFML syntaxes");

    let ss = builder.build();
    for warning in ss.warnings() {
        println!("cargo:warning={}", warning);
    }

    let unlinked = ss.find_unlinked_contexts();
    for message in &unlinked {
        println!("cargo:warning={}", message);
    }
    if !unlinked.is_empty() {
        panic!("syntax pack has unresolved context references");
    }

    let required = [
        "embedding.cfml",
        "source.cfml.script",
        "text.html.cfml",
        "source.java",
        "source.sql",
        "source.css",
        "source.js.plain",
        "source.json",
    ];
    for scope in &required {
        if ss
            .find_syntax_by_scope(Scope::new(scope).unwrap())
            .is_none()
        {
            panic!("required scope {} missing from syntax pack", scope);
        }
    }

    dump_to_uncompressed_file(&ss, Path::new(&out_dir).join("cftokens.packdump"))
        .expect("write cftokens.packdump");
}
