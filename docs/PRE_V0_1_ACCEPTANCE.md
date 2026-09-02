# Pre-v0.1 acceptance contract

This document separates source, runtime, field, and release truth. A higher level includes all earlier requirements; passing a source suite never silently advances installation, enforcement, dogfood, or publication.

Levels 1–4 have historical evidence at the exact admitted point. Accepted generation 13 and the schema-v4 App are installed but deliberately contained report-only. The current process-only source candidate must independently pass source/CI, transactional rollback/reinstall, installed full-timing acceptance and final arm before it becomes the active level-4 implementation. Levels 5–7 remain separate future owner decisions.

## Claim levels

| Level | Name | Required gates and runtime | Architecture and admitted matrix | Field / rollback evidence | Allowed claim | Known residuals |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Source complete | Rust fmt/clippy/workspace tests/release build; source-only doctor and dry-run; Swift tests and bundle verification; schema v1/v3/v4, migrations, fixtures, privacy/docs gates | macOS 14+ source; Apple-silicon development evidence; exact matrix in [`support-matrix.v1.json`](support-matrix.v1.json) | no installed mutation; no field claim required | “pre-v0.1 source candidate” | all field, architecture, signing and release gates remain |
| 2 | Isolated report-only verified | Level 1 plus repeatable smoke using only a unique temp database/socket/lock, effective report-only, v4 reads/mutations/receipts/restart reconciliation, private modes, and no IP listener | same narrow matrix; no ambient eligibility expansion | isolated restart/reconnect proof; cleanup removes only its owned temp root | “pre-v0.1 source candidate — isolated report-only verified” | installed/ambient/multi-day/enforce, Intel/universal, signing/release remain |
| 3 | Installed report-only integrated | Level 2 plus a packaged schema-v4 App and v4/v3 daemon installed without arm; exact generation/service/App readback; App and daemon restart reconciliation | explicitly named installed architecture and unchanged process/artifact matrix | candidate-specific lease; exact restart; real rollback to the prior healthy report-only generation; fresh reinstall before acceptance | “pre-v0.1 installed report-only candidate” | no enforcement, ambient, public or broad support claim |
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

The candidate daemon migrates stores to SQLite schema v6 while the retained generation 9 understands schema v5 only. The acceptance-scoped lease retains the prior manifest, report-only plist and SQLite snapshot after candidate readiness. It blocks install, uninstall and mode changes until explicit accept or rollback, and provides an exact `restart-report-only` acceptance path.

[`INSTALLED_DOGFOOD.md`](INSTALLED_DOGFOOD.md) first passed with a real rollback to generation 9 and its v5 database, exact old-binary healthy readback, generation-12 reinstall, and packaged v3 App/daemon restart reconciliation. Generation 13 later installed the v4 App/runtime and passed a controlled level-4 process/artifact/restart run, but its own new rollback lease was accepted without being executed. The authorized process-only replacement must therefore run the complete candidate-specific install → restart → rollback to generation 13 → fresh reinstall → restart → accept sequence before its managed field run and final arm. Historical level-4 evidence does not waive that transaction.
