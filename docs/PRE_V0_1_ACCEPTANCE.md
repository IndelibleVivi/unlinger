# Pre-v0.1 acceptance contract

This document separates source, runtime, field, and release truth. A higher level includes all earlier requirements; passing a source suite never silently advances installation, enforcement, dogfood, or publication.

Levels 1 and 2 are verified. The owner has now authorized work toward level 3 inside the exact report-only runbook; this does not authorize enforcement. Levels 4–7 remain separate future owner decisions.

## Claim levels

| Level | Name | Required gates and runtime | Architecture and admitted matrix | Field / rollback evidence | Allowed claim | Known residuals |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Source complete | Rust fmt/clippy/workspace tests/release build; source-only doctor and dry-run; Swift tests and bundle verification; schema v1/v3, migrations, fixtures, privacy/docs gates | macOS 14+ source; Apple-silicon development evidence; exact matrix in [`support-matrix.v1.json`](support-matrix.v1.json) | no installed mutation; no field claim required | “pre-v0.1 source candidate” | all field, artifact, architecture, signing and release gates remain |
| 2 | Isolated report-only verified | Level 1 plus repeatable smoke using only a unique temp database/socket/lock, effective report-only, v3 reads/mutations/receipts/restart reconciliation, private modes, and no IP listener | same narrow matrix; no ambient eligibility expansion | isolated restart/reconnect proof; cleanup removes only its owned temp root | “pre-v0.1 source candidate — isolated report-only verified” | installed v3, ambient/multi-day/enforce, both artifact P2s, Intel/universal, signing/release remain |
| 3 | Installed report-only integrated | Level 2 plus a packaged v3 App and v3 daemon installed without arm; exact generation/service/App readback; App and daemon restart reconciliation | explicitly named installed architecture and unchanged process/artifact matrix | acceptance-scoped rollback lease retains prior manifest/plist/v5 DB until explicit accept; real rollback restores generation 9 healthy ready-report-only and its binary opens the restored DB | “pre-v0.1 installed report-only candidate” | no enforcement, ambient, public or broad support claim |
| 4 | Private enforcement candidate | Level 3 plus explicit owner authorization for the exact managed field boundary; deterministic gates and final report-only containment | only the exact admitted CfT version/family shape actually exercised | narrow full-timing process/artifact/restart receipt and verified rollback/containment | “private enforcement candidate for the exact admitted point” | no ambient or v0.1 acceptance; artifact P2s must be named unless resolved |
| 5 | Private v0.1 accepted | sustained private dogfood, zero unexplained signals, owner acceptance, release rollback, and resolved/accepted artifact-risk decision | every advertised architecture and exact support row verified | multi-day ambient evidence, a real eligible incident, narrow enforce acceptance, recovery/rollback evidence | “private v0.1 accepted” within the exact documented support matrix | any accepted residual must be explicit and user-visible |
| 6 | Public alpha | Level 5 plus public-safe repository, rights/license decision, signed/notarized distribution candidate, public docs and support channel | published matrix exactly matches shipped artifacts | public-alpha telemetry-free operational evidence and rollback procedure | “public alpha” | alpha limitations and open support rows remain prominent |
| 7 | Public release | all advertised product, platform, signing/notarization, distribution/update, rollback, privacy and support gates accepted | shipped universal/architecture claims backed by artifacts | release acceptance and repeatable rollback/update evidence | “Unlinger v0.1 public release” | only explicitly accepted release residuals |

## Non-substitution rules

- A green build is source evidence, not installation evidence.
- An isolated report-only daemon is not the installed service.
- A controlled harness-created CfT incident is not ambient dogfood.
- One successful `DevToolsActivePort` removal does not close either artifact P2.
- An ad-hoc-signed private bundle is not Developer ID signing or notarization.
- A private remote or pushed commit is not a release.

## Current gate for level 3

The source daemon migrates stores to SQLite schema v6 while installed generation 9 understands schema v5 only. Source now implements an acceptance-scoped lease that retains the prior manifest, report-only plist and SQLite snapshot after candidate readiness. It blocks install, uninstall and mode changes until explicit accept or rollback, and provides an exact `restart-report-only` acceptance path.

Level 3 remains unproved until [`INSTALLED_DOGFOOD.md`](INSTALLED_DOGFOOD.md) actually restores generation 9 and its v5 database, the exact old CLI/daemon returns healthy ReadyReportOnly, the same exact-head candidate is reinstalled, and the packaged v3 App passes installed/restart reconciliation. Until then the strongest permissible claim remains level 2.
