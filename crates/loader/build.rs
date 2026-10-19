//! Generates one forwarding stub per POWRPROF.dll export and exports it
//! under the original name.

use std::fmt::Write;

fn main() {
    println!("cargo:rerun-if-changed=powrprof.exports");
    let names: Vec<String> = std::fs::read_to_string("powrprof.exports")
        .expect("powrprof.exports")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();

    let mut stubs = String::new();
    writeln!(
        stubs,
        "pub(crate) const EXPORTS: [&str; {}] = [",
        names.len()
    )
    .unwrap();
    for name in &names {
        writeln!(stubs, "    {name:?},").unwrap();
    }
    writeln!(stubs, "];").unwrap();
    writeln!(stubs, "core::arch::global_asm!(").unwrap();
    for (i, name) in names.iter().enumerate() {
        writeln!(stubs, "    \".globl ensh_stub_{name}\",").unwrap();
        writeln!(stubs, "    \"ensh_stub_{name}:\",").unwrap();
        writeln!(
            stubs,
            "    \"jmp qword ptr [rip + {{table}} + {}]\",",
            i * 8
        )
        .unwrap();
    }
    writeln!(stubs, "    table = sym crate::proxy::FORWARDS,").unwrap();
    writeln!(stubs, ");").unwrap();
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("stubs.rs");
    std::fs::write(out, stubs).unwrap();

    if std::env::var("TARGET").unwrap().ends_with("windows-msvc") {
        for name in &names {
            println!("cargo:rustc-cdylib-link-arg=/EXPORT:{name}=ensh_stub_{name}");
        }
    }
}
