use std::path::{Path, PathBuf};
#[path = "../../src/po.rs"]
mod po;
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    println!("cargo:rustc-env=NOWBAR_SOURCE_ROOT={}", root.display());
    for (language, name) in [("en", "EN"), ("ar", "AR")] {
        let path = root
            .join("i18n")
            .join(language)
            .join("LC_MESSAGES/nowbar-ui.po");
        println!("cargo:rerun-if-changed={}", path.display());
        let mut entries = po::parse_po(&std::fs::read_to_string(path).unwrap());
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.dedup_by(|a, b| a.0 == b.0);
        let rows: String = entries
            .iter()
            .map(|(key, text)| format!("({key:?}, {text:?}),\n"))
            .collect();
        std::fs::write(
            out.join(format!("{language}.rs")),
            format!("static {name}: &[(&str, &str)] = &[{rows}];"),
        )
        .unwrap();
    }
}
