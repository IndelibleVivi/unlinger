# Signature packs

Signature packs are versioned TOML data compiled into the binary. In 0.1 they parameterize one shared deterministic Rust sessionizer for agent-browser, Playwright, and Puppeteer Chromium sessions. A schema-v2 pack supplies controller, framework, ephemeral-profile, browser-root, and recorder markers together with a graceful strategy, browser version policy, and runtime-artifact policy. Packs are data, not per-framework graph programs or executable cleanup logic.

A signature is evidence, not authorization. The shared classifier applies the same graph construction, counterevidence, age, product/version, controller, profile, debug-peer, isolation, cooling, and revalidation rules to every pack. It requires multiple independent provenance categories and a framework-specific anchor; generic `Chrome`, `--headless`, `--remote-debugging-port`, age, PPID, CPU, or RSS is insufficient on its own. Candidate age below 60 seconds, an unavailable age, or a wall-clock underflow is a hard protection and never positive evidence.

Browser-root executable markers apply to the executable basename, not to an enclosing app-bundle or cache path. Helpers such as `chrome_crashpad_handler` may remain members of a reconstructed tree, but an app path containing `Google Chrome for Testing` or `ms-playwright` cannot make such a helper a standalone browser-root candidate.

Automatic browser eligibility is currently an exact private dogfood point only:

- bundle identifier `com.google.chrome.for.testing`;
- `CFBundleShortVersionString` exactly `151.0.7922.34`;
- version facts collected from the exact `.app/Contents/MacOS/<executable>` bundle without shelling out;
- every browser root in the incident agrees on that product and version.

Missing, mixed, wrong-product, or wrong-version facts produce `PROTECTED`, not a best-effort match. Controller version is not yet independently proven by the v2 packs, so any candidate that still contains a controller is also `PROTECTED`, including a reparented controller. Current automatic eligibility is therefore limited to a detached controllerless browser session that passes every remaining gate. This is not a supported-version range.

Every pack change must include:

1. one redacted positive fixture;
2. the nearest plausible normal or manually controlled counterexample;
3. the supported product/version statement, with an exact allowlist or evidenced bounded range;
4. controller-version evidence if the change would make controller-bearing candidates eligible;
5. a safety explanation for any new automatic-eligibility or artifact path.

Remote executable rule updates are out of scope. The tracked `rules/*.toml` files are canonical source; `include_str!` embeds their exact bytes at compile time.

All three current packs use `os_term_only`, admit `ffmpeg` only through the shared exact direct-controller-child/process-group join, and set `devtools_active_port = true`. A plausible same-user, no-TTY recorder sharing the browser process group but not admitted into the incident protects the whole candidate rather than leaving residue behind.

The artifact flag does not authorize a generic profile sweep. It can produce at most one transient `DevToolsActivePort` candidate for an otherwise eligible, exact-version, single-profile session. The cleanup engine still requires the tree to be gone with no revival, a targeted Darwin reference query for the exact canonical and quarantine pathnames, a complete current-user argv pass, exact file/parent identity, and a durable PREPARED artifact-action row before unlink. Sockets, PID files, profiles, and directories have no current automatic eligibility. Crash recovery after the quarantine rename and the final same-UID `fstatat`/`unlinkat` TOCTOU remain known P2 boundaries, so one controlled successful DAP removal must not be presented as broad artifact acceptance.

Historical field evidence includes detached ephemeral Playwright-style sessions using Chrome for Testing 151.0.7922.34. It supports that single browser point only. Earlier managed attempts failed closed at zombie, transport, old artifact-scan, empty-argv, counterexample-proof, or read-only polling boundaries. Generation 9 then passed one complete installed managed run with an eight-member exact tree, zero survivors, both revival checks, an exact DAP removal, fresh-epoch restart without resend, and ordinary-root preservation. This remains one exact product/version field point, not a supported range or controller-bearing admission.
