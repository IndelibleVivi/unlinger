# Current state

**Updated:** 2026-09-08. **Programme:** 0.1. **Reader posture:** experimental developer source preview; not a signed/notarized App release or multi-day reliability claim.

## Source, remote and installed state

| Surface | Observed truth |
| --- | --- |
| Runtime implementation | `b70bc94b3d0aba4cac2db1bbe924c2666ac725b8`: task lifetime plus inherited-descriptor launch fix |
| Subsequent test correction | `f22e08eb3410050b380eaee7b84c4ccda2a3ad1a`: bounded post-release offline-lock tests; production locking/timeouts unchanged |
| Baseline remote verification | [CI 34163955192](https://github.com/IndelibleVivi/unlinger/actions/runs/34163955192) passed for `b70bc94`; [CI 34165331792](https://github.com/IndelibleVivi/unlinger/actions/runs/34165331792) passed for `f22e08e`, including default-parallel Rust tests, release build, Swift tests and App bundling |
| Current reader preparation | Bilingual READMEs, first-use/install/uninstall guide, current architecture, material provenance and safe demo teardown; local validation complete; hosted CI/public switch pending |
| Maintainer's reference service | Accepted generation 23 from `b70bc94`, healthy `ReadyEnforce`, no pending candidate lease; this is one reference installation, not a generation number users should copy |
| Reference protocols/persistence | Operator v1, frontend v5/v4/v3, SQLite v8; historical v2 rejected |
| Reference App | Unchanged ad-hoc-signed schema-v5 App assembled from `016ca58`; neither Developer ID signed nor notarized |
| Policy | Playwright `0.5.0`, agent-browser/Puppeteer `0.4.0`; process-only; every artifact flag false |
| Publication | Repository private pending candidate checks; SUL-1.0 + CC BY-NC-SA 4.0 selected; no GitHub Release |

The source daemon defaults to report-only. The reference service's explicit activation is separate from that default and from building the source. Public reader preparation does not replace or restart that service/App.

## Current controlled evidence

Generation 22 installed report-only, passed the seven App socket tests before and after restart, then **actually rolled back** to generation 19 with its exact old CLI/daemon reopening SQLite v7. A fresh generation 23 repeated installation/restart/App checks, was accepted, and was explicitly armed. Earlier initial-source candidates were rolled back before final acceptance and do not supply final-binary acceptance.

Two deliberately created task-owned CfT `152.0.7977.42` sessions then reached terminal process-only receipts on generation 23:

| Result | Controlled task 1 | Controlled task 2 |
| --- | --- | --- |
| Delivered action | One controller TERM | One controller TERM |
| Processes | 8 → 0 | 8 → 0 |
| Estimated memory impact | 91,979,776 bytes | 164,691,968 bytes |
| Revival checks | 2 | 2 |
| Runtime-artifact actions | 0 | 0 |
| Independent unrelated-session check | Verifier stopped at an intermediate record; later terminal readback proved cleanup, not complete control-session acceptance | Separate ordinary session still evaluated `42`, then closed normally by the verifier |

Independent action/impact authority agrees on 16 reclaimed processes and 256,671,744 estimated memory bytes. Actual installed-App readback showed **2 sessions, 16 processes and 244.8 MB**; its memory display uses binary scaling despite the MB label. The separate 1.48-GB clone card is **observed logical disk size**, not freed storage. Private profiles, raw evidence and user paths remain outside Git.

Isolated full-timing task tests cover exact CfT `151.0.7922.34` and `152.0.7977.42`, including active-owner/client protection, terminal cleanup and an unrelated session still usable. These are controlled points, not ordinary ambient eligible cleanup or multi-day dogfood. The complete managed restart/containment Level-4 harness remains historical generation-15 evidence; it is not attributed to generation 23.


The publication candidate passed fresh local formatting, strict workspace clippy, **313 Rust tests** (three explicit live-browser cases ignored), **81 Swift tests**, App bundle checks, and both isolated report-only App/socket passes (**7 + 7**). The new demo supervisor passed its two owned-child regression checks. A clean checkout with matching staged source built the release workspace; its source-only doctor was healthy, dry-run stayed nonmutating, and the one-shot preview produced zero cleanup receipts. The interactive reader example waited for healthy readiness, ran `/usr/bin/true` once, reached `released` with exit 0, and produced no cleanup impact. This verifies the documented source preview, not first installation on a separate clean macOS account. The architecture SVG was rendered and visually inspected; reader links and bilingual command/version/license parity were checked. The unchanged Accessibility/RSS lane was not repeated for these reader/script changes.

## Known limits and verification gaps

- The original generation-17 terminal SQLite disk-I/O failure cause remains unproved. Exact-instance containment/recovery and later transactional replacement succeeded; a later healthy database check does not establish the original cause.
- The new command wrapper does not integrate every Codex App host or browser tool automatically. Exact CLI/browser compatibility, inherited session, lifetime/client proof and all ordinary gates remain required. Unregistered or unverified controllers stay protected.
- Chrome clone observation now accepts the actual `.app.bundle` shape and no-follow framework links. A real observation matched one clone and 1,475,187,528 regular-file logical bytes; live references were observed and deletion remains unavailable.
- All artifact admission is disabled. The dormant DAP engine still has a quarantine-after-crash recovery gap and a final pathname-swap TOCTOU. Native pathname-reference tests also intermittently returned no reference for an owned open ordinary or `O_EVTONLY` descriptor under parallel execution; exact serial tests passed, and the cause is unresolved. The active process path does not use that query. [Safety](SAFETY.md) owns these boundaries.
- The old zero-deadline offline-lock test failed because a concurrent fork can inherit an `O_CLOEXEC` descriptor until exec. A deterministic owned-child probe established that cause; `f22e08e` retains held-lock denial and gives post-release acquisition its existing bounded wait. The final exact-head CI passed. A later local full workspace run reproduced the separate dormant native-query failures above; no assertions were weakened.
- The latest fixture Accessibility/RSS attempt could not acquire its target window and **did not pass**. Trusted desktop Accessibility separately read the actual installed App. Historical repaired-App evidence includes ten minutes/1,398 tree reads and a later bounded v5 deployment observation; those do not establish multi-day App behavior, packaged notifications or every menu organizer/display arrangement.
- No Intel/universal verification, signed/notarized distribution, automatic update path or public release is claimed. Recognition of agent-browser/Puppeteer is not controlled field acceptance.

## Publication preparation

The preparation baseline `f22e08e` scan covered 205 current text files in each working/index/ref view and 756 reachable historical text blobs. Its 25 scope-repeated findings were reviewed as synthetic test/fixture paths and notification event-token code. The automated scan skipped two current and six historical binary assets; they were separately inspected as generated source images and derived icons, including metadata and historical provenance. This is a bounded review, not a secrets-free certificate.

A historical frontend handoff was reviewed in full as a technical interface/implementation guide, without private chats, local personal paths or account data. It was removed from the current tree in `df161af`. No history rewrite or separate repository is needed on the inspected evidence. [Provenance](PROVENANCE.md) records material boundaries without copying private working notes.

The owner selected the license scope in [LICENSING.md](../LICENSING.md), confirmed rights authority and authorized public visibility after candidate checks. That visibility change is pending. The owner-approved source/content licenses are present. Reader documentation and the architecture export do not imply a released App or completed binary-distribution review. See [acceptance levels](PRE_V0_1_ACCEPTANCE.md), [support](SUPPORT.md) and the [support matrix](support-matrix.v1.json) for claim vocabulary.
