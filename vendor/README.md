# vendor: VectorCraft crates patched by ArtServer

English | [日本語](README.ja.md)

To fix a bug in VectorCraft, only the affected crate is copied here and fixed, and it replaces the original through
`[patch."https://github.com/storytold/vectorcraft"]` in `Cargo.toml` (docs/design-policy.md, chapter 8).
VectorCraft crates that are not here come from GitHub at the commit given in `Cargo.toml`.

No crate is patched at present (since 2026-10-25), and `Cargo.toml` has no `[patch."https://github.com/storytold/vectorcraft"]`.

| Crate | Original commit | Fix | Diff |
|---|---|---|---|
| (none) | | | |

## Patches removed

| Crate | Fix | Used | Why it was removed |
|---|---|---|---|
| `vectorcraft-svg` | Japanese font weights (W0–W9) were not written to SVG as `font-weight`, so Illustrator showed W6 in the regular weight ([#2873](https://github.com/storytold/vectorcraft/issues/2873)) | 2026-10-18 to 2026-10-25, with VectorCraft v0.13.1 | Upstream fixed it at the root ([#2896](https://github.com/storytold/vectorcraft/pull/2896)): `style_weight` in `vectorcraft-text` reads W0–W9 as the digit × 100, at least 100, the same weights as the patch. It is in releases from v0.14.0, and ArtServer moved to v0.14.0 when the patch was removed |

The copy, its diff and the notes on it are in the Git history (`git log -- vendor/vectorcraft-svg`).

## How to make a copy

- Copy `crates/<name>/src` of the commit ArtServer references as is, fix it, and keep the diff from the original
  in `patches/<crate>.diff`.
- The original `Cargo.toml` inherits the workspace settings (`workspace = true`), so write the values from the same commit
  directly. The other VectorCraft crates point to GitHub at the same commit as ArtServer.
- Tests (`tests/` and the dev-dependencies) are not needed, because ArtServer's build doesn't use them.
- Copy `LICENSE-MIT` and `LICENSE-APACHE` from the root of the VectorCraft repository (MIT OR Apache-2.0), and make the crate's
  entry in `about.toml` read them from the copy (`files` instead of `git`).
- Add the crate to the table above, and write the symptom, the cause, the fix, its scope, how it was checked and the upstream report below it.

## When updating VectorCraft

After raising the commit in ArtServer's `Cargo.toml`, move the crates here to the same commit. VectorCraft's
crates call each other's internal functions, so the build fails if their commits differ.

1. If upstream has merged the fix, delete the crate from here and remove its line from `[patch]` in `Cargo.toml`
   (move it to "Patches removed" above, and point its entry in `about.toml` back to `git`)
2. If not, copy `crates/<name>/src` of the new commit again and apply `patches/<crate>.diff`
   (if it doesn't apply, read the code, fix it, and regenerate the diff)
3. Match the commits and values of the dependencies in `Cargo.toml` to the new commit's workspace settings
4. Run `cargo test`, `cargo deny check`, and the license notice generation (see the README)
5. Update "Original commit" in the table above

If more crates need fixes and reapplying them becomes a burden, switch to a fork of VectorCraft (docs/design-policy.md, chapter 8).
