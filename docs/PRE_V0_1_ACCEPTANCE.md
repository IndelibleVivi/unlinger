# Pre-v0.1 acceptance contract

This document separates source, runtime, field, and release truth. A higher level includes all earlier requirements; passing a source suite never silently advances installation, enforcement, dogfood, or publication.

Levels 1–4 were verified for the exact historical generation-15/schema-v4/SQLite-v6/`0.3.0` CfT-151 point. Current schema-v5/SQLite-v7/`0.4.0` generation 19 independently passed Level 3 through generation-18 install/restart/real rollback to generation 17/v7, fresh generation-19 reinstall, and current App/restart/live-observation checks. It was accepted and explicitly armed. It has no candidate-specific controlled signal run or ambient eligible cleanup, so generation 15's Level-4 receipt cannot be borrowed as generation-19 or CfT-152 acceptance. Multi-day dogfood, an ordinary ambient eligible incident and owner private-v0.1 acceptance remain open. Levels 5–7 remain separate future owner decisions; [`current-state.md`](current-state.md) owns current activation truth.

## Claim levels

| Level | Name | Required gates and runtime | Architecture and admitted matrix | Field / rollback evidence | Allowed claim | Known residuals |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Source complete | Rust fmt/clippy/workspace tests/release build; source-only doctor and dry-run; Swift tests and bundle verification; current operator/frontend schemas, migrations, fixtures, privacy/docs gates | macOS 14+ source; Apple-silicon development evidence; exact matrix in [`support-matrix.v1.json`](support-matrix.v1.json) | no installed mutation; no field claim required | “pre-v0.1 source candidate” | all field, architecture, signing and release gates remain |
| 2 | Isolated report-only verified | Level 1 plus repeatable smoke using only a unique temp database/socket/lock, effective report-only, current-schema reads/mutations/receipts/restart reconciliation, private modes, and no IP listener | same narrow matrix; no ambient eligibility expansion | isolated restart/reconnect proof; cleanup removes only its owned temp root | “pre-v0.1 source candidate — isolated report-only verified” | installed/ambient/multi-day/enforce, Intel/universal, signing/release remain |
| 3 | Installed report-only integrated | Level 2 plus the candidate's packaged current-schema App and daemon installed without arm; exact generation/service/App readback; retained compatibility checks; App and daemon restart reconciliation | explicitly named installed architecture and unchanged process/artifact matrix | candidate-specific lease; exact restart; real rollback to the prior healthy report-only generation; fresh reinstall before acceptance | “pre-v0.1 installed report-only candidate” | no enforcement, ambient, public or broad support claim |
| 4 | Private enforcement candidate | Level 3 plus explicit owner authorization for the exact managed field boundary; deterministic gates and final report-only containment | only the exact admitted CfT version/family shape actually exercised; current policy may be process-only | narrow full-timing process/restart receipt, zero artifact actions when process-only, and verified rollback/containment | “private enforcement candidate for the exact admitted point” | no ambient or v0.1 acceptance; dormant artifact P2s remain named before any re-enable |
| 5 | Private v0.1 accepted | sustained private dogfood, zero unexplained signals, owner acceptance, release rollback, and resolved/accepted artifact-risk decision | every advertised architecture and exact support row verified | multi-day ambient evidence, a real eligible incident, narrow enforce acceptance, recovery/rollback evidence | “private v0.1 accepted” within the exact documented support matrix | any accepted residual must be explicit and user-visible |
| 6 | Public alpha | Level 5 plus public-safe repository, rights/license decision, signed/notarized distribution candidate, public docs and support channel | published matrix exactly matches shipped artifacts | public-alpha telemetry-free operational evidence and rollback procedure | “public alpha” | alpha limitations and open support rows remain prominent |
| 7 | Public release | all advertised product, platform, signing/notarization, distribution/update, rollback, privacy and support gates accepted | shipped universal/architecture claims backed by artifacts | release acceptance and repeatable rollback/update evidence | “Unlinger v0.1 public release” | only explicitly accepted release residuals |

## Non-substitution rules

- A green build is source evidence, not installation evidence.
- An isolated report-only daemon is not the installed service.
- A controlled harness-created CfT incident is not ambient dogfood.
- Historical successful `DevToolsActivePort` removals do not close either artifact P2 or authorize current artifact admission.
- An ad-hoc-signed private bundle is not Developer ID signing or notarization.
- A private remote or pushed commit is not a release.

## Level-3 evidence and current gate

The exercised generation-12/15/17 transactions migrated stores across SQLite schema boundaries while their retained prior generations understood older schemas. Their acceptance-scoped leases retained the prior manifest, report-only plist and SQLite snapshot after candidate readiness, blocked install/uninstall/mode changes until explicit accept or rollback, and provided an exact `restart-report-only` acceptance path.

[`INSTALLED_DOGFOOD.md`](INSTALLED_DOGFOOD.md) first passed with a real rollback to generation 9 and its v5 database, exact old-binary healthy readback, generation-12 reinstall, and packaged v3 App/daemon restart reconciliation. Generation 13 later installed the v4 App/runtime and passed a controlled level-4 process/artifact/restart run, but its own lease was accepted without being executed. The process-only replacement closed that gap: generation 14 installed/restarted report-only and really rolled back to healthy generation 13; the same exact-head candidate reinstalled as generation 15, repeated restart/App checks, was accepted, passed a full-timing process-only run with zero artifact actions, returned report-only, then was explicitly armed and remained healthy through a later completed sweep.

The SQLite-v7/schema-v5 tranche completed that Level-3 lane on 2026-09-04. Generation 16 retained the exact generation-15 SQLite-v6 snapshot and really rolled back; the old generation reopened v6 healthy and report-only. The same exact candidate then freshly reinstalled as generation 17 and repeated packaged schema-v5 App/live-socket checks before and after restart. Existing generation-15 signal evidence still cannot substitute for generation-17 Level-4 evidence.
