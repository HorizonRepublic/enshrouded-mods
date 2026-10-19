//! Prints a binary's build fingerprint and the entry RVAs of named systems.
//! Usage: fingerprint <exe> [system...]

use keen_reflect::{Image, Layout, Reflection};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: fingerprint <exe> [system...]");
    let bytes = std::fs::read(&path).expect("readable binary");
    let image = Image::parse(&bytes, Layout::File, None).expect("a PE image");
    match image.fingerprint {
        Some(f) => {
            let guid: String = f.guid.iter().map(|b| format!("{b:02x}")).collect();
            println!("fingerprint {guid}/{}", f.age);
        }
        None => println!("fingerprint none"),
    }
    let reflection = Reflection::scan(&image);
    for name in args {
        match reflection.system(&name) {
            Ok(s) => println!("{name} {:#x}", s.entry.saturating_sub(image.base)),
            Err(e) => println!("{name}: {e}"),
        }
    }
}
