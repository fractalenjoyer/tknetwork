use regex::Regex;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let name = env::var("CARGO_PKG_NAME").unwrap();
    let dest_path = Path::new(&out_dir).join("module.rs");
    let source = fs::read_to_string("src/lib.rs").unwrap();

    let functions = Regex::new(r"#\[pyfunction]\s*(?:\w\s+)*?fn\s+([\w0-9]+)").unwrap();
    let structs = Regex::new(r"#\[pyclass]\s*(?:\w\s+)*?(?:struct|enum)\s+([\w0-9]+)").unwrap();

    fs::write(dest_path, format!("#[pymodule]
    fn {}(_py: Python, m: &PyModule) -> PyResult<()> {{\n", name)
        + &functions
            .captures_iter(&source)
            .map(|f| format!(
                "m.add_function(wrap_pyfunction!({}, m)?)?;\n", &f[1]))
            .collect::<String>()
        + &structs
            .captures_iter(&source)
            .map(|s| format!(
                "m.add_class::<{}>()?;\n", &s[1]))
            .collect::<String>()
        + "Ok(())}").unwrap();
}