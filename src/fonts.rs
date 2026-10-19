//! The fonts this server has only as built-in copies. VectorCraft compiles a few Latin families into the
//! program (feature `bundled-fonts`: Source Sans 3, Source Serif 4, Inter and JetBrains Mono), so the server
//! always has them, while the apps people edit exported files in usually don't (docs/design-policy.md
//! section 7.2). `list_fonts` marks them so that the AI can prefer installed families.

use std::sync::OnceLock;

use vectorcraft_text::FontDb;

/// The families built into the server and not installed on its OS, worked out once.
pub fn bundled_only() -> &'static [String] {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        let db = FontDb::global();
        built_in().into_iter().filter(|family| !installed(db, family)).collect()
    })
}

/// The families built into the program: a database without font folders holds only those.
fn built_in() -> Vec<String> {
    FontDb::with_font_dirs(Vec::new()).families()
}

/// Is some style of `family` read from a font file (installed on the OS, or in `--font-dirs`)?
fn installed(db: &FontDb, family: &str) -> bool {
    db.styles(family)
        .iter()
        .filter_map(|style| db.face(family, style))
        .any(|face| db.versions(&face).iter().any(|v| v.path().is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_families_are_vectorcraft_bundled_fonts() {
        // Notices when a VectorCraft update changes the bundled fonts
        let built_in = built_in();
        for family in ["Source Sans 3", "Source Serif 4", "Inter", "JetBrains Mono"] {
            assert!(built_in.iter().any(|f| f == family), "{family} is not built in: {built_in:?}");
        }
        // Without font files, nothing counts as installed
        let empty = FontDb::with_font_dirs(Vec::new());
        assert!(!installed(&empty, "Inter"));
        assert!(bundled_only().iter().all(|f| built_in.contains(f)), "{:?}", bundled_only());
    }
}
