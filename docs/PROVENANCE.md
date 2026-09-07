# Material provenance

This document describes incorporated material. The owner confirmed authority to grant the project material under the exact scopes in [LICENSING.md](../LICENSING.md); that license map, not Git authorship alone, defines the grant.

| Material | Observed provenance | Boundary |
| --- | --- | --- |
| Rust workspace, Swift App, scripts, rules, fixtures and documentation | Project development history; the reachable history at `f22e08e` has one recorded author, Faye Fang | Git authorship is evidence of contribution history, not a copyright assignment or a guarantee about every line |
| `apps/UnlingerApp/AssetsSource/appicon-source.png` | Project-requested AI-generated image introduced in `df161af`; embedded content credentials identify OpenAI Media Service and `gpt-image` | Generated image, not a hand-authored vector; no claim that AI generation guarantees exclusivity |
| `apps/UnlingerApp/Resources/AppIcon.icns` | Derived from that source through macOS image resizing/iconset compilation | Same image provenance; compiled packaging adds no independent rights grant |
| Retired menu-bar source and PNGs in Git history | The other project-requested generated image, also introduced in `df161af`; derived template PNGs removed in `32fd467` | Historical assets remain reachable; current App uses the system `circle.dashed` symbol |
| `docs/architecture.mmd` and `docs/architecture.svg` | Project-authored architecture model; SVG rendered with Mermaid | Diagram of inspected source; no imported artwork |
| Canonical fixtures | Synthetic or deliberately redacted process topology and protocol data | No browser contents, credentials or real profile identifiers are intended for these files |

Rust dependencies are resolved by `Cargo.lock` from their own upstream packages; they retain their own licenses. SwiftPM declares no external package dependency. This source tree does not vendor browser binaries or browser profiles. Building/downloading dependencies and distributing a compiled App are different publication scopes; dependency notices must be reconciled for any future binary distribution.

The Apple system status symbol is selected through the operating system; it is not a copied bundled image. Chrome, Playwright and other referenced product names identify compatibility and imply no affiliation.

The publication preparation includes a reachable-history text scan and manual review of the binary assets, metadata and historical technical handoff. A pattern scan is not a proof that a repository contains no sensitive information. Exact scan scope and unresolved publication decisions live in [current state](current-state.md).
