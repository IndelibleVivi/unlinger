# Signature packs

Signature packs are versioned TOML data compiled into the binary. In 0.1 they may identify agent-browser, Playwright, and Puppeteer Chromium sessions. A pack describes controller markers, framework-specific runtime/profile markers, browser executable markers, automation transports, headless flags, and hard profile protections.

A signature is evidence, not authorization. The classifier requires multiple independent provenance categories and a framework-specific anchor; generic `Chrome`, `--headless`, `--remote-debugging-port`, age, and PPID are insufficient on their own.

Browser-root executable markers apply to the executable basename, not to an enclosing app-bundle or cache path. Helpers such as `chrome_crashpad_handler` may remain members of a reconstructed tree, but an app path containing `Google Chrome for Testing` or `ms-playwright` cannot make such a helper a standalone browser-root candidate.

Every pack change must include:

1. one redacted positive fixture;
2. the nearest plausible normal or manually controlled counterexample;
3. the supported runtime/version statement, or `unknown` when the pack is observational rather than version-proven;
4. a safety explanation for any new automatic-eligibility path.

Remote executable rule updates are out of scope. The tracked `rules/*.toml` files are canonical source; `include_str!` embeds their exact bytes at compile time.

Current field evidence includes one detached ephemeral Playwright-style session using Chrome for Testing 151.0.7922.34. That is a single verified point, not a supported-version range; the pack remains labelled observational until the broader version matrix passes.
