//! Checks a game executable against the build table.
//!
//! Usage: `check_build <exe>`. Prints a Markdown report on stdout and exits
//! with 0 when the build is known and everything the mods use still
//! resolves, 1 when a known build no longer matches its row, and 2 when the
//! build is new.

use keen_reflect::{Image, Layout, Reflection};
use powrprof::builds::{self, Role};

/// Fields the hooks read or write, with the offsets they rely on.
const FIELDS: &[(&str, &str, u64)] = &[
    ("keen::ecs::TriggerShape", "rangeX", 4),
    ("keen::ecs::TriggerShape", "rangeY", 8),
    ("keen::ecs::TriggerShape", "rangeZ", 0xc),
];

fn main() {
    let path = std::env::args().nth(1).expect("usage: check_build <exe>");
    let bytes = std::fs::read(&path).expect("readable executable");
    let image = Image::parse(&bytes, Layout::File, None).expect("a PE image");
    let reflection = Reflection::scan(&image);
    let rva = |entry: u64| entry.saturating_sub(image.base);

    let fingerprint = image.fingerprint.map(|f| {
        let guid: String = f.guid.iter().map(|b| format!("{b:02x}")).collect();
        format!("{guid}/{}", f.age)
    });
    let role = match reflection.system("add_buildzones_client") {
        Ok(s) if s.entry != 0 => Role::Client,
        _ => Role::Server,
    };
    println!("## {role:?} executable");
    println!();
    println!(
        "- fingerprint: `{}`",
        fingerprint.as_deref().unwrap_or("none")
    );
    println!(
        "- {} types, {} systems",
        reflection.type_count(),
        reflection.system_count()
    );

    let mut broken = Vec::new();
    for &(ty, field, offset) in FIELDS {
        match reflection.field_offset(ty, field) {
            Ok(found) if found == offset => {}
            Ok(found) => broken.push(format!(
                "`{ty}.{field}` moved from {offset:#x} to {found:#x}"
            )),
            Err(e) => broken.push(format!("`{ty}.{field}`: {e}")),
        }
    }

    let known = builds::find(image.fingerprint);
    // A new build is compared with the newest row of the same role.
    let reference = known.or_else(|| builds::BUILDS.iter().rev().find(|b| b.role == role));
    if let Some(row) = reference {
        println!();
        println!("| System | {} | This executable |", row.name);
        println!("| --- | --- | --- |");
        for &(name, expected) in row.systems {
            let found = reflection.system(name).map(|s| rva(s.entry));
            let shown = match &found {
                Ok(r) => format!("{r:#x}"),
                Err(e) => e.to_string(),
            };
            println!("| `{name}` | {expected:#x} | {shown} |");
            if known.is_some() && found.as_ref().ok() != Some(&expected) {
                broken.push(format!("system `{name}` no longer at {expected:#x}"));
            }
        }
    }

    println!();
    let code = match (known, broken.is_empty()) {
        (Some(row), true) => {
            println!(
                "Known build **{}**: everything the mods use resolves.",
                row.name
            );
            0
        }
        (Some(row), false) => {
            println!("Known build **{}**, but:", row.name);
            broken.iter().for_each(|b| println!("- {b}"));
            1
        }
        (None, _) => {
            println!(
                "**New build.** The mods run vanilla on it until the build table has a row for it."
            );
            broken.iter().for_each(|b| println!("- {b}"));
            2
        }
    };
    std::process::exit(code);
}
