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
| PC-01..04 | Sections 3-4 | install-and-forget, silent, deterministic, local/private | private per-user LaunchAgent active in enforce dogfood after report-only persistence and mode rollback; silent success log; redacted persistence | sustained dogfood, unhealthy-user signal, release acceptance | partial |
| DET-01 | 6.1 | four evidence families with explicit contrary evidence | typed evidence ledger and positive/nearest-counterexample corpus | real supported-family capture | verified at fixture boundary |
| DET-02 | 6.2 | every hard gate; score never authorizes cleanup | gate truth test, two observations, durable grace, exact revalidation, one isolated real CfT cleanup | broader live false-positive and race evidence | implemented |
| DET-03 | 6.3 | complete confidence-state model | guarded transitions plus daemon cleanup outcomes | persistence-corruption and long dogfood transitions | implemented |
| RUN-01 | 7.1-7.2 | startup/periodic/exit/wake/pressure triggers and cooling defaults | launchd-owned startup + persistent periodic cycles, 15 s stability, durable 90 s grace, 120 s continuity reset | exit dispatch, wake, pressure acceleration | partial |
| RUN-02 | 7.3 | TERM, rescan, exact KILL, post-scan, bounded revival | frozen executor, synthetic ordering/race tests, owned-child macOS signal test, isolated CfT TERM/KILL/zero-survivor receipt, active private enforce mode | first ambient eligible incident, broader real framework trees, and full chaos matrix | implemented, active private dogfood |
| RUN-03 | 7.4 | remove only proven stale low-risk metadata; never profiles in 0.1 | no deletion path exists, so profiles remain untouched | reference-aware stale socket/PID metadata cleanup | planned |
| ARCH-01 | 8.1-8.2 | Rust core/rules/macos/daemon/CLI separation | workspace builds, tests, and clippy pass | publication-grade bilingual diagram later | verified at source boundary |
| MAC-01 | 8.3 | native current-user snapshots and stable identity | libproc/sysctl snapshot, exact lookup/signal, latest recorded installed-host doctor 386/386 in 58 ms | CPU/descriptor facts, event sources, Intel/universal verification | partial |
| STORE-01 | 8.4 | bounded SQLite receipts and local read-mostly IPC | schema migration guard, redacted typed events with terminal completion time, 14 d/10k prune, durable cooling/pause, 0600 blocking socket, two 40-request/concurrency-8 installed-service bursts | corruption recovery and sustained load evidence | implemented |
| RULE-01 | 8.5 | embedded versioned packs with positive and nearest counterexamples | three validated embedded TOML packs, eight-case corpus, crashpad-root regression, one CfT 151.0.7922.34 field point | supported-version range evidence | implemented, observational versions |
| SCOPE-01 | 9 | macOS 14+, current user, three Chromium families only | platform/UID checks and scoped packs | Intel/universal and real family matrix | partial |
| CLI-01 | 10 | status/history/explain/doctor/pause/resume/dry-run/export | complete ordinary commands plus transactional service install/status/set-mode/uninstall, parser tests, installed-service IPC acceptance | broader user acceptance and packaged command discovery | implemented |
| SAFE-01..13 | 11 | listed safety invariants | source guards, synthetic counterexamples, exact owned-child test, terminal failure receipts, full-timing isolated cleanup, scoped fast harness, ambient protected-browser coexistence, graceful shutdown/no-new-cleanup gate | artifact rules, chaos completion, sustained dogfood/public-alpha zero-FP evidence | partial |
| ACC-01 | 12 | latency, safety, completeness, overhead targets | latest recorded installed-host doctor 58 ms; full-timing and fast isolated cleanups ended with zero survivors/no revival; first installed enforce point was 0.0% CPU and 720 KiB RSS with no IP socket | sustained benchmark, representative latency/completeness, multi-day dogfood and alpha | in progress |
| TEST-01 | 13.1-13.4 | fixture, live, adversarial, comparative evidence | 46 passing ordinary Rust tests plus one ignored live CfT path, report-only and enforce LaunchAgent cycles, transactional mode rollback, two installed IPC bursts, full-timing and fast isolated enforcement runs | broader real corpus, remaining chaos, benchmarks, comparative Field Lab | in progress |
| DIST-01 | Phase 3 | universal signed/notarized release, Homebrew, update/rollback | rollback-capable private per-user binary/plist transaction with same-directory per-file promotion; operational mode rollback exercised | universal build, signing identity, notarization, packaging, power-loss-safe versioned upgrade/rollback and Homebrew evidence | in progress |
| SURF-01 | Phase 4 | optional UI, signed data rules, Linux/Windows, new families | none | separately admitted product work | planned |

## Scope and order deltas

- **Reordered, no product delta:** the private per-user install transaction, activation-failure rollback, and mode rollback mechanics from `DIST-01` were implemented during P2 dogfood before universal packaging, signing/notarization, and Homebrew. The owner explicitly authorized installation and ambient dogfood on the first Mac. This advances a reversible implementation dependency; it does not remove or lower any P3 release gate.
- No accepted requirement was added, removed, or narrowed.

## Dependency order and current tranche

1. **P0 Evidence Lab — source largely implemented:** native facts → identity/graph → rules → counterexamples → read-only live smoke. The real incident corpus remains open.
2. **P1 Deterministic cleanup — source candidate implemented:** durable cooling → frozen plan → exact revalidation → TERM/rescan/exact KILL → post-scan/revival → terminal receipt. One isolated CfT run passed; the broader framework and live chaos matrix remains open.
3. **P2 Ambient daemon — active private dogfood:** periodic scheduler, SQLite, blocking IPC, daemon, CLI, per-user LaunchAgent lifecycle, report-only persistence, enforce activation, and operational mode rollback have passed on one Mac; event sources and sustained dogfood remain open.
4. **P3 Distribution hardening — started at private-install boundary:** same-directory per-file promotion and activation-failure rollback exist; universal release → signing/notarization → Homebrew → power-loss-safe versioned update/rollback → public alpha evidence remain.
5. **P4 Optional surfaces/platforms — planned:** thin UI projection, signed data-only rules, Linux/Windows, separately admitted families.

The owner explicitly asked not to stop at a minimal report-only implementation, so work advanced through P1 into active private P2 dogfood and the first private-install part of P3. This changes implementation sequencing, not the acceptance threshold: activation receipts remain distinct from sustained safety, eligible-incident, release, and public acceptance.

## Private-remote and publication gates

The first remote is private. Private provenance, live captures, diagnostics, local continuity, machine paths, and personal working documents remain outside Git. A visibility change is a separate owner action and requires bilingual reader documentation, a publication-grade architecture diagram, license/rights selection, real field evidence, public-safe examples, and a fresh tracked/staged privacy scan.

## Full acceptance

Unlinger 0.1 is complete only when every ledger row through P3 is verified with the appropriate source, built artifact, activated runtime, safety, dogfood, and release evidence. A green fixture suite, exact owned-child signal test, private remote, or report-only smoke is not auto-cleanup readiness.
