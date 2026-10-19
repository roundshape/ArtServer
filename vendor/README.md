# vendor: VectorCraft crates patched by ArtServer

English | [日本語](README.ja.md)

To fix a bug in VectorCraft, only the affected crate is copied here and fixed, and it replaces the original through
`[patch."https://github.com/storytold/vectorcraft"]` in `Cargo.toml` (docs/design-policy.md, chapter 8).
VectorCraft crates that are not here come from GitHub at the commit given in `Cargo.toml`.

| Crate | Original commit | Fix | Diff |
|---|---|---|---|
| `vectorcraft-svg` | `904f0fe5227bd987d55319e89a38b4b8f046998c` (v0.13.1) | Font weight of text in SVG (below) | `patches/vectorcraft-svg.diff` |

## vectorcraft-svg: Japanese font weights (W0–W9) are not written to SVG

- **Symptom**: Text set in, for example, Hiragino Kaku Gothic W6 is exported to SVG without `font-weight`,
  and Illustrator and other apps show it in the regular weight (such as W3). PDF and SVG with outlined text are correct.
- **Cause**: The SVG export derives the weight from the style name with `vectorcraft_text::style_weight`.
  That function reads only English words (Light, Medium, Bold and so on), so "W6" becomes 400 (regular),
  and for 400 no `font-weight` is written (`font_props` in `src/css.rs`).
- **Fix**: `font_descriptor` in `src/css.rs` reads W0–W9 as the digit × 100
  (W3 is 300, W6 is 600; W0 and W1 are 100). Other style names still go through `style_weight`.
  When a variable font's `wght` axis is set, that value is used first (as upstream v0.13.1 does).
  The weights in the `@font-face` rules of embedded fonts use the same function, so they are fixed too.
- **Scope**: Only SVG export (and CSS export). Font selection (`vectorcraft-text`) is unchanged.
- **Verification**: The fixed SVG was opened in Illustrator, and Hiragino Kaku Gothic W6 and Hiragino Mincho W6 were shown in the W6 weight (2026-10-18).
- **Reported upstream**: [storytold/vectorcraft#2873](https://github.com/storytold/vectorcraft/issues/2873) (2026-10-18).
  The report suggests, as the root fix, reading W0–W9 in `style_weight` of `vectorcraft-text` (which would also help font selection).
  Once upstream fixes it, remove this patch.
- **Reapplying**: When ArtServer moved to v0.13.1 on 2026-10-18, upstream's `font_descriptor` had changed to check a variable font's `wght` axis
  first, so the fix was reapplied by reading the code instead of applying the diff. #2873 is still not fixed in v0.13.1.

## How the copy was made

- `crates/svg/src` of the original commit was copied as is, and the diff in `patches/` was applied.
- The original `Cargo.toml` inherits the workspace settings (`workspace = true`), so the values from the same commit
  are written directly. The other VectorCraft crates point to GitHub at the same commit as ArtServer.
- Tests (`tests/` and the dev-dependencies) are not included because ArtServer's build doesn't use them.
- Since v0.13.1 the crate depends on `craft-docpath` (craft-libs, rev `82a24b0`), which was added to `Cargo.toml`.
- `LICENSE-MIT` and `LICENSE-APACHE` are the ones at the root of the VectorCraft repository (MIT OR Apache-2.0). They are unchanged in v0.13.1.

## When updating VectorCraft

After raising the commit in ArtServer's `Cargo.toml`, move the crates here to the same commit. VectorCraft's
crates call each other's internal functions, so the build fails if their commits differ.

1. If upstream has merged the fix, delete the crate from here and remove its line from `[patch]` in `Cargo.toml`
2. If not, copy `crates/<name>/src` of the new commit again and apply `patches/<crate>.diff`
   (if it doesn't apply, read the code, fix it, and regenerate the diff)
3. Match the commits and values of the dependencies in `Cargo.toml` to the new commit's workspace settings
4. Run `cargo test`, `cargo deny check`, and the license notice generation (see the README)
5. Update "Original commit" in the table above

If more crates need fixes and reapplying them becomes a burden, switch to a fork of VectorCraft (docs/design-policy.md, chapter 8).
