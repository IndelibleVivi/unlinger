# Privacy

Unlinger is local-only and has no account, telemetry, cloud sync, hosted report, remote rule fetch, or normal-operation network traffic.

Process arguments and executable paths can contain usernames, repositories, profile names, URLs, ports, and tokens. The macOS backend reads them transiently because exact classification requires flags and path conventions. They remain inside the in-memory snapshot/analyzer/cleanup boundary.

The SQLite schema persists typed redacted observations and cleanup receipts: incident IDs, pack/version, process roles and counts, basenames, resource totals, evidence IDs, hard-gate values, redacted fingerprints, signal stages/dispositions, and bounded outcomes. It does not serialize raw argv, executable paths, profile paths, frozen target identities, tracking keys, or session fingerprints into user-visible history. The private cooling table retains only redacted fingerprints and timestamps needed to prove abandonment grace.

The database file and Unix socket are created with mode 0600; newly created Unlinger parent directories use mode 0700. CLI history/explain and diagnostic export project the same redacted persisted types. A diagnostic output file uses 0600 and refuses to replace an existing path unless the caller explicitly passes `--force`.

Fixtures use synthetic paths and identifiers. Page contents, browser history, cookies, credentials, application conversations, saved authentication state, and the private origin conversation are outside the observation and repository contracts.

Private working continuity and live/private captures stay outside Git. Repository privacy does not make them safe to commit. Before any public visibility change, tracked content must be scanned again for machine paths, personal identifiers, raw captures, private provenance, secrets, and unsupported product claims.
