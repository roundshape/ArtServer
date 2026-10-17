# Third-Party Notices

ArtServer itself is licensed under the MIT License (see `LICENSE`). It depends on the components
below, which keep their own licenses. ArtServer does not relicense any of them.

> **Status:** this file summarises the obligations found in the Phase 0 audit (`docs/architecture.md`
> §2.9). Before ArtServer is distributed as a binary or container image, a complete notice file with
> the full license texts of every compiled-in crate must be generated (e.g. with `cargo about`) and
> shipped with it. Distributing source that refers to these components through Cargo does not copy them.

## VectorCraft

- Source: https://github.com/storytold/vectorcraft (commit `fdac986bb49b3e3a9ddd3cda4e88661a6f0aa50c`)
- Crates used: `vectorcraft-engine`, `vectorcraft-render`, `vectorcraft-text` and the internal crates they depend on
- License: MIT OR Apache-2.0
- Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors

ArtServer is based on VectorCraft by the ArtCraft team. It is not an official VectorCraft derivative
and is not endorsed by the ArtCraft team. The ArtCraft name, wordmark and mark are not open source and
are not used by ArtServer (VectorCraft `docs/brand/LICENSE-brand.txt`).

## craft-libs (PDF backend)

- Source: https://github.com/storytold/craft-libs (rev `ca50d63b773c99f96e75c77ce5a1d21e7c9cda0d`)
- Components: `craft-pdf`, and the patched `krilla`, `hayro-syntax`, `hayro-interpret`, `hayro-jbig2`
- License: MIT OR Apache-2.0
- Copyright (c) 2026 ArtCraft Team and the craft-libs contributors; Copyright (c) The Hayro Authors

Compiled into the PDF reader:

- **Adobe CMap resources** — BSD-3-Clause, Copyright Adobe. The names of Adobe and its contributors
  may not be used to endorse or promote ArtServer.
- **Foxit / PDFium standard fonts** — BSD-3-Clause, Copyright The PDFium Authors.
- **ICC profiles shipped with krilla** — CC0.

The full texts are in VectorCraft's `NOTICE` and `licenses/pdf-backend/`.

## Fonts bundled by vectorcraft-text

Compiled in through the `bundled-fonts` feature. All are licensed under the SIL Open Font License 1.1.

| Font | Reserved Font Name |
|---|---|
| Source Sans 3 (Regular, Semibold, Bold, Italic) | "Source" (Adobe) |
| Source Serif 4 (Regular) | "Source" (Adobe) |
| Inter (Regular, Medium, SemiBold) | — |
| JetBrains Mono (Regular) | — |

The license texts are VectorCraft's `assets/fonts/OFL-*.txt`. The fonts may not be sold by themselves.
Fonts embedded in exported documents are permitted by the OFL.

## Other notable licenses in the dependency tree

| Component | License |
|---|---|
| jpeg-encoder | (MIT OR Apache-2.0) AND IJG — "this software is based in part on the work of the Independent JPEG Group" |
| ICU4X crates (`icu_*`, `zerovec`, `yoke`, `tinystr`, …) | Unicode-3.0 |
| brotli | BSD-3-Clause AND MIT |
| tiny-skia-path | BSD-3-Clause |
| arrayref | BSD-2-Clause |
| foldhash, zlib-rs, slotmap | Zlib |
| moxcms, pxfm | BSD-3-Clause OR Apache-2.0 |
| ryu | Apache-2.0 OR BSL-1.0 |
| rmcp (MCP Rust SDK), axum, tokio and most other crates | MIT and/or Apache-2.0 |

No dependency of ArtServer is licensed under the GPL, LGPL, MPL or a proprietary license (static audit
of `Cargo.lock`; to be confirmed with `cargo deny check licenses`).
