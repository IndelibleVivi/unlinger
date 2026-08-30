# Signature packs

Signature packs are versioned TOML data compiled into the binary. In 0.1 they may identify agent-browser, Playwright, and Puppeteer Chromium sessions. A pack describes controller markers, framework-specific runtime/profile markers, browser executable markers, automation transports, headless flags, and hard profile protections.

A signature is evidence, not authorization. The classifier requires multiple independent provenance categories and a framework-specific anchor; generic `Chrome`, `--headless`, `--remote-debugging-port`, age, and PPID are insufficient on their own.

Every pack change must include:

1. one redacted positive fixture;
2. the nearest plausible normal or manually controlled counterexample;
3. the supported runtime/version statement, or `unknown` when the pack is observational rather than version-proven;
4. a safety explanation for any new automatic-eligibility path.

Remote executable rule updates are out of scope. The tracked `rules/*.toml` files are canonical source; `include_str!` embeds their exact bytes at compile time.
