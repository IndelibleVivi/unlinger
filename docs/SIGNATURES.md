# Signature packs

Signature packs are versioned TOML data compiled into the binary. In 0.1 they parameterize one shared deterministic Rust sessionizer for agent-browser, Playwright, and Puppeteer Chromium sessions. A schema-v2 pack supplies controller, framework, ephemeral-profile, browser-root, and recorder markers together with a graceful strategy, browser version policy, and runtime-artifact policy. Packs are data, not per-framework graph programs or executable cleanup logic.

A signature is evidence, not authorization. The shared classifier applies the same graph construction, counterevidence, age, product/version, controller, profile, debug-peer, isolation, cooling, and revalidation rules to every pack. It requires multiple independent provenance categories and a framework-specific anchor; generic `Chrome`, `--headless`, `--remote-debugging-port`, age, PPID, CPU, or RSS is insufficient on its own. Candidate age below 60 seconds, an unavailable age, or a wall-clock underflow is a hard protection and never positive evidence.

Browser-root executable markers apply to the executable basename, not to an enclosing app-bundle or cache path. Helpers such as `chrome_crashpad_handler` may remain members of a reconstructed tree, but an app path containing `Google Chrome for Testing` or `ms-playwright` cannot make such a helper a standalone browser-root candidate.

Automatic browser eligibility is an exact source allowlist only:

- bundle identifier `com.google.chrome.for.testing`;
- `CFBundleShortVersionString` exactly `151.0.7922.34` or `152.0.7977.42`;
- version facts collected from the exact `.app/Contents/MacOS/<executable>` bundle without shelling out;
- every browser root in the incident agrees on that product and version.

Missing, mixed, wrong-product, or wrong-version facts produce `PROTECTED`. The source Playwright `0.5.0` pack additionally supplies `task_controller_version = "1.63.0-alpha-2026-08-31"`. It admits a controller only when the native package/registry/socket facts match that version and an exact daemon-issued task binding. Task release, complete client visibility and no active task work are required in addition to every existing browser/protection gate. Unregistered or unverified controllers remain protected. Agent-browser and Puppeteer remain `0.4.0` and controllerless. The installed policy is recorded separately in [current-state.md](current-state.md). See [task ownership](TASKS.md).

Every pack change must include:

1. one redacted positive fixture;
2. the nearest plausible normal or manually controlled counterexample;
3. the supported product/version statement, with an exact allowlist or evidenced bounded range;
4. controller-version evidence if the change would make controller-bearing candidates eligible;
5. a safety explanation for any new automatic-eligibility or artifact path.

Remote executable rule updates are out of scope. The tracked `rules/*.toml` files are canonical source; `include_str!` embeds their exact bytes at compile time.

All current source and installed packs use `os_term_only`, admit `ffmpeg` only through the shared exact direct-controller-child/process-group join, and set `devtools_active_port = false`. Source also supports the exact task-owned Playwright CLI lane above; browser versions and all artifact flags are unchanged by that addition. A plausible same-user, no-TTY recorder sharing the browser process group but not admitted into the incident protects the whole candidate rather than leaving residue behind. The analyzer produces no runtime-artifact candidate.

The artifact flag does not authorize a generic profile sweep. If a future pack re-enables it, it can produce at most one transient `DevToolsActivePort` candidate for an otherwise eligible, exact-version, single-profile session. The dormant cleanup engine still requires the tree to be gone with no revival, a targeted Darwin reference query for the exact canonical and quarantine pathnames, a complete current-user argv pass, exact file/parent identity, and a durable PREPARED artifact-action row before unlink. Sockets, PID files, profiles, and directories have no automatic eligibility. Crash recovery after the quarantine rename and the final same-UID `fstatat`/`unlinkat` TOCTOU remain known P2 boundaries; re-enabling the flag requires their resolution or explicit owner acceptance.

Historical field evidence includes detached ephemeral Playwright-style sessions using Chrome for Testing 151.0.7922.34. It supports that single browser point only; installed CfT `152.0.7977.42` currently has fixture evidence plus an ambient protected observation because controller-version authority is incomplete, but no controlled cleanup evidence. Earlier managed attempts failed closed at zombie, transport, old artifact-scan, empty-argv, counterexample-proof, or read-only polling boundaries. Generations 9 and 13 later passed controlled installed runs with eight-member exact trees, zero survivors, both revival checks, exact DAP removals, fresh-epoch restart without resend, and ordinary-root preservation. Those runs predate the current process-only policy and remain historical evidence, not current artifact authority, a supported range, or controller-bearing admission.
