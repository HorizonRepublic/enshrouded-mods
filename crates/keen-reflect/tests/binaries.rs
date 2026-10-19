//! Checks against the real game binaries. Keen's binaries never enter the
//! repository, so these run only where `ENSH_SERVER_EXE` and
//! `ENSH_CLIENT_EXE` point at local copies, and pass trivially elsewhere.

use keen_reflect::{Image, Layout, Reflection, fnv1a32};

#[test]
fn fnv1a32_matches_the_reference_values() {
    assert_eq!(fnv1a32(""), 0x811c_9dc5);
    assert_eq!(fnv1a32("a"), 0xe40c_292c);
    assert_eq!(fnv1a32("foobar"), 0xbf9c_f968);
}

fn scan(var: &str) -> Option<(Vec<u8>, Reflection)> {
    let path = std::env::var(var).ok()?;
    let bytes = std::fs::read(path).expect("binary readable");
    let reflection = {
        let image = Image::parse(&bytes, Layout::File, None).expect("a PE image");
        Reflection::scan(&image)
    };
    Some((bytes, reflection))
}

/// What every mod needs from either binary.
fn check_common(reflection: &Reflection) {
    let shape = "keen::ecs::TriggerShape";
    assert_eq!(reflection.field_offset(shape, "rangeX"), Ok(4));
    assert_eq!(reflection.field_offset(shape, "rangeY"), Ok(8));
    assert_eq!(reflection.field_offset(shape, "rangeZ"), Ok(0xc));
    assert!(reflection.type_info("keen::ecs::BaseIdComponent").is_ok());
    assert!(reflection.type_info("keen::BalancingTable").is_ok());

    let upgrade = reflection.system("upgrade_building_range").unwrap();
    assert_ne!(upgrade.entry, 0);
    assert!(
        upgrade.writes.iter().any(|c| c == "TriggerShape"),
        "{upgrade:?}"
    );
    assert!(reflection.system("add_buildzones_server").is_ok());
}

#[test]
fn server_binary_resolves() {
    let Some((bytes, reflection)) = scan("ENSH_SERVER_EXE") else {
        return;
    };
    check_common(&reflection);
    let image = Image::parse(&bytes, Layout::File, None).unwrap();
    assert!(image.fingerprint.is_some());
    eprintln!(
        "server: {} types, {} systems, fingerprint {:?}",
        reflection.type_count(),
        reflection.system_count(),
        image.fingerprint
    );
}

#[test]
fn client_binary_resolves() {
    let Some((_, reflection)) = scan("ENSH_CLIENT_EXE") else {
        return;
    };
    check_common(&reflection);
    assert_ne!(reflection.system("add_buildzones_client").unwrap().entry, 0);
}
