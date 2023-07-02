use std::{
    ffi::OsStr,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use anyhow::bail;
use clap::Parser;
use wasmparser::Encoding;

#[derive(Parser)]
struct Args {
    /// The module of the import to look for (usually "env").
    module: String,
    /// The name of the import to look for.
    name: String,
    /// The path to the `deps` folder within your `target` folder (usually
    /// either `target/wasm32-unknown-unknown/debug/deps` or
    /// `target/wasm32-unknown-unknown/release/deps`).
    deps_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let Args {
        module,
        name,
        deps_path,
    } = Args::parse();

    let mut found_rlibs = Vec::new();
    for entry in fs::read_dir(deps_path)? {
        let path = entry?.path();
        if path.extension() == Some(OsStr::new("rlib")) {
            if rlib_contains_import(&path, &module, &name)? {
                found_rlibs.push(path);
            }
        }
    }

    if found_rlibs.is_empty() {
        println!("No imports of \"{module}\" \"{name}\" found.");
    } else {
        println!("\"{module}\" \"{name}\" is imported by:");
        for rlib in found_rlibs {
            println!("  {}", rlib.display());
        }
    }

    Ok(())
}

fn rlib_contains_import(path: &Path, module: &str, name: &str) -> anyhow::Result<bool> {
    let mut archive = ar::Archive::new(File::open(path)?);
    while let Some(entry) = archive.next_entry() {
        let mut entry = entry?;
        match wasm_contains_import(&mut entry, module, name) {
            // If we couldn't parse the wasm, it's probably just because it isn't a wasm file; not
            // only do we not filter this to only `.o` files, but it's possible to have non-wasm
            // dependencies in a wasm project (e.g. build scripts, proc macros).
            Err(_) => {}
            Ok(true) => return Ok(true),
            Ok(false) => {}
        }
    }
    Ok(false)
}

fn wasm_contains_import(mut wasm: impl io::Read, module: &str, name: &str) -> anyhow::Result<bool> {
    let mut buffer = Vec::new();
    wasm.read_to_end(&mut buffer)?;

    let parser = wasmparser::Parser::new(0);

    for payload in parser.parse_all(&buffer) {
        match payload? {
            wasmparser::Payload::Version { encoding, .. } => {
                if encoding != Encoding::Module {
                    bail!("found a wasm object file which wasn't a module")
                }
            }
            wasmparser::Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import?;
                    if import.module == module && import.name == name {
                        // We found a matching import!
                        return Ok(true);
                    }
                }
            }
            wasmparser::Payload::End(_) => return Ok(false),
            _ => {}
        }
    }

    bail!("ran out of payloads without hitting `Payload::End`")
}
