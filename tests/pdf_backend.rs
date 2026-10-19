//! Checks that PDF reading and writing use the fixed backends from craft-libs (docs/design-policy.md chapter 8).
//!
//! Cargo doesn't carry a dependency's `[patch]` over, so `Cargo.toml` repeats VectorCraft's `[patch.crates-io]`.
//! If it stops applying, say after VectorCraft moves to another craft-libs revision, Cargo only warns and takes the
//! crates.io versions, which lack the limits on broken PDFs. craft-pdf refuses to build with an unfixed writer
//! (krilla), but nothing stops an unfixed reader (hayro-*). VectorCraft checks the same with `cargo xtask pdf-deps`.

const LOCK: &str = include_str!("../Cargo.lock");

/// The crates `[patch.crates-io]` replaces. Each must resolve once, from the revision craft-pdf comes from
const PATCHED: &[&str] = &["krilla", "hayro-syntax", "hayro-interpret", "hayro-jbig2"];

/// The sources of the packages named `name` in a lockfile ("none" for a path dependency)
fn sources<'a>(lock: &'a str, name: &str) -> Vec<&'a str> {
    let name_line = format!("name = \"{name}\"");
    lock.split("[[package]]")
        .filter(|package| package.lines().any(|line| line == name_line))
        .map(|package| {
            package
                .lines()
                .find_map(|line| line.strip_prefix("source = \"")?.strip_suffix('"'))
                .unwrap_or("none")
        })
        .collect()
}

fn check(lock: &str) -> Result<(), String> {
    let pdf = match sources(lock, "craft-pdf")[..] {
        [source] if source.starts_with("git+https://github.com/storytold/craft-libs?rev=") => source,
        ref found => return Err(format!("craft-pdf: expected one from craft-libs, found {found:?}")),
    };
    for name in PATCHED {
        let found = sources(lock, name);
        if found != [pdf] {
            return Err(format!("{name}: expected one from {pdf}, found {found:?}"));
        }
    }
    Ok(())
}

#[test]
fn the_pdf_backends_come_from_craft_pdfs_revision() {
    assert_eq!(check(LOCK), Ok(()));
}

#[test]
fn an_unfixed_doubled_or_missing_backend_is_noticed() {
    const FIXED: &str = "git+https://github.com/storytold/craft-libs?rev=a#a";
    const OTHER: &str = "git+https://github.com/storytold/craft-libs?rev=b#b";
    const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
    let lock = |packages: &[(&str, &str)]| -> String {
        packages
            .iter()
            .map(|(name, source)| format!("[[package]]\nname = \"{name}\"\nversion = \"0.1.0\"\nsource = \"{source}\"\n\n"))
            .collect()
    };
    let fixed: Vec<_> = std::iter::once("craft-pdf")
        .chain(PATCHED.iter().copied())
        .map(|name| (name, FIXED))
        .collect();
    let moved = |name: &str, source| -> Vec<_> { fixed.iter().map(|&(n, s)| (n, if n == name { source } else { s })).collect() };
    assert_eq!(check(&lock(&fixed)), Ok(()));

    for name in PATCHED {
        // From crates.io, as when the patch stops applying, or from another revision than craft-pdf
        for source in [REGISTRY, OTHER] {
            assert!(check(&lock(&moved(name, source))).is_err(), "{name} from {source}");
        }
        // Both versions in the build
        let mut doubled = fixed.clone();
        doubled.push((name, REGISTRY));
        assert!(check(&lock(&doubled)).is_err(), "{name} twice");
        // Not in the build at all
        let missing: Vec<_> = fixed.iter().copied().filter(|(n, _)| n != name).collect();
        assert!(check(&lock(&missing)).is_err(), "{name} missing");
    }
    assert!(check(&lock(&moved("craft-pdf", REGISTRY))).is_err(), "craft-pdf from crates.io");
}
