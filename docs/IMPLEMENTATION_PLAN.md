# Unlinger 0.1 implementation plan

## Spec authority

- Canonical specification: [`docs/SPEC.md`](SPEC.md), Product & Technical Specification 0.1, dated 2026-08-30.
- Approval provenance: the project owner supplied the specification and explicitly asked to start the independent `unlinger` repository. The document remains labelled `Working draft`; product deltas still require owner confirmation.
- Normative companions: none. The origin conversation is private explanatory provenance and is neither copied nor normative.
- Repository baseline: initialized from an empty directory on local `main`; a private remote is configured.

Status terms: `verified` means the named evidence boundary passed; `implemented` means the source path exists with focused verification but its later field/activation boundary remains open; `partial` names an intentionally unfinished row rather than hiding it.

## Complete coverage ledger

| ID | Spec anchor | Intended outcome | Current evidence | Remaining boundary | Status |
| --- | --- | --- | --- | --- | --- |
| PC-01..04 | Sections 3-4 | install-and-forget, silent, deterministic, local/private | local-only source, default report-only daemon, redacted persistence | LaunchAgent dogfood, silent failure behavior, release acceptance | partial |
| DET-01 | 6.1 | four evidence families with explicit contrary evidence | typed evidence ledger and positive/nearest-counterexample corpus | real supported-family capture | verified at fixture boundary |
| DET-02 | 6.2 | every hard gate; score never authorizes cleanup | gate truth test, two observations, durable grace, exact revalidation, one isolated real CfT cleanup | broader live false-positive and race evidence | implemented |
| DET-03 | 6.3 | complete confidence-state model | guarded transitions plus daemon cleanup outcomes | persistence-corruption and long dogfood transitions | implemented |
| RUN-01 | 7.1-7.2 | startup/periodic/exit/wake/pressure triggers and cooling defaults | startup + periodic cycles, 15 s stability, durable 90 s grace, 120 s continuity reset | exit dispatch, wake, pressure acceleration | partial |
| RUN-02 | 7.3 | TERM, rescan, exact KILL, post-scan, bounded revival | frozen executor, synthetic ordering/race tests, owned-child macOS signal test, isolated CfT TERM/KILL/zero-survivor receipt | broader real framework trees and full chaos matrix | implemented, not activated |
| RUN-03 | 7.4 | remove only proven stale low-risk metadata; never profiles in 0.1 | no deletion path exists, so profiles remain untouched | reference-aware stale socket/PID metadata cleanup | planned |
| ARCH-01 | 8.1-8.2 | Rust core/rules/macos/daemon/CLI separation | workspace builds, tests, and clippy pass | publication-grade bilingual diagram later | verified at source boundary |
| MAC-01 | 8.3 | native current-user snapshots and stable identity | libproc/sysctl snapshot, exact lookup/signal, live 452/452 doctor | CPU/descriptor facts, event sources, Intel/universal verification | partial |
| STORE-01 | 8.4 | bounded SQLite receipts and local read-mostly IPC | schema migration guard, redacted typed events with terminal completion time, 14 d/10k prune, durable cooling/pause, 0600 socket round-trip | corruption recovery and long-running load evidence | implemented |
| RULE-01 | 8.5 | embedded versioned packs with positive and nearest counterexamples | three validated embedded TOML packs, eight-case corpus, crashpad-root regression, one CfT 151.0.7922.34 field point | supported-version range evidence | implemented, observational versions |
| SCOPE-01 | 9 | macOS 14+, current user, three Chromium families only | platform/UID checks and scoped packs | Intel/universal and real family matrix | partial |
| CLI-01 | 10 | status/history/explain/doctor/pause/resume/dry-run/export | complete commands, parser tests, real local IPC smoke | installed-service/user acceptance | implemented |
| SAFE-01..13 | 11 | listed safety invariants | source guards, synthetic counterexamples, exact owned-child test, terminal failure receipts, one isolated cleanup with simultaneous ordinary Chrome | artifact rules, chaos completion, sustained dogfood/public-alpha zero-FP evidence | partial |
| ACC-01 | 12 | latency, safety, completeness, overhead targets | 28 ms live read-only snapshot; one isolated cleanup ended with zero survivors and no revival | sustained benchmark, representative latency/completeness, dogfood and alpha | in progress |
| TEST-01 | 13.1-13.4 | fixture, live, adversarial, comparative evidence | 33 Rust tests, report-only daemon/CLI smoke, and one isolated CfT enforcement run | broader real corpus, remaining chaos, benchmarks, comparative Field Lab | in progress |
| DIST-01 | Phase 3 | universal signed/notarized release, Homebrew, update/rollback | none | signing identity, packaging, install/upgrade/rollback evidence | planned |
| SURF-01 | Phase 4 | optional UI, signed data rules, Linux/Windows, new families | none | separately admitted product work | planned |

## Dependency order and current tranche

1. **P0 Evidence Lab — source largely implemented:** native facts → identity/graph → rules → counterexamples → read-only live smoke. The real incident corpus remains open.
2. **P1 Deterministic cleanup — source candidate implemented:** durable cooling → frozen plan → exact revalidation → TERM/rescan/exact KILL → post-scan/revival → terminal receipt. One isolated CfT run passed; the broader framework and live chaos matrix remains open.
3. **P2 Ambient daemon — partially implemented:** periodic scheduler, SQLite, IPC, daemon, and CLI exist; one isolated source-run has passed, while event sources, LaunchAgent packaging, activation, and sustained dogfood remain open.
4. **P3 Distribution hardening — planned:** universal release → signing/notarization → Homebrew → atomic update/rollback → public alpha evidence.
5. **P4 Optional surfaces/platforms — planned:** thin UI projection, signed data-only rules, Linux/Windows, separately admitted families.

The owner explicitly asked not to stop at a minimal report-only implementation, so reversible source work advanced through P1 and part of P2. This changes implementation sequencing, not the acceptance threshold: no source or synthetic result is treated as ambient activation proof.

## Private-remote and publication gates

The first remote is private. Private provenance, live captures, diagnostics, local continuity, machine paths, and personal working documents remain outside Git. A visibility change is a separate owner action and requires bilingual reader documentation, a publication-grade architecture diagram, license/rights selection, real field evidence, public-safe examples, and a fresh tracked/staged privacy scan.

## Full acceptance

Unlinger 0.1 is complete only when every ledger row through P3 is verified with the appropriate source, built artifact, activated runtime, safety, dogfood, and release evidence. A green fixture suite, exact owned-child signal test, private remote, or report-only smoke is not auto-cleanup readiness.
