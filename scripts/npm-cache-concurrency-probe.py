#!/usr/bin/env python3
"""Reproduce the npm 11.19.0 / bundled cacache 20.0.4 concurrent-use blocker.

Hypothesis (from source review): the native `cacache.verify()` maintenance entry
point deletes the whole `_cacache/tmp` directory as its final `cleanTmp` step,
while concurrent npm writers hold live request-owned work directories in
`_cacache/tmp` (e.g. the bundled pacote Git fetcher clones into
`cacache.tmp.withTmp()`). A verify interleaved with such a writer destroys the
writer's live temporary tree. The blocker is deterministic, not a data race
that needs repetition.

This probe is negative-admission evidence only. Exit zero means the unsafe,
version-exact macOS interleaving was reproduced; it does NOT admit npm cache
maintenance for automatic runtime cleanup.

Safety:
* Python standard library only.
* Every path lives under a fresh, test-owned temporary root; the real user
  cache, the installed service, and the installed App are never touched.
* No network: the producer guard `npm config get offline` must be `true`, so
  the only git fetch is a local `git+file://` dependency (same machine).
* No package downloads and no installed dependencies; only the explicit
  absolute `node`, npm CLI, and bundled `cacache` paths the caller passes are
  used.

The probe pairs a control case (the writer completes with no maintenance
interleaved) with an interference case (the native maintenance entry point runs
while the writer is paused, holding a populated live temporary tree).
"""

import argparse
import json
import pathlib
import shlex
import subprocess
import sys
import tempfile
import time


# The exact unsafe producer API, confined by this probe's newly-created cache.
NATIVE_VERIFY = r"""
const cacache = require(process.argv[1]);
cacache.verify(process.argv[2]).then(s => {
  const result = {reclaimedCount:s.reclaimedCount, reclaimedSize:s.reclaimedSize,
                  badContentCount:s.badContentCount};
  if (!Object.values(result).every(n => Number.isSafeInteger(n) && n >= 0)) process.exit(7);
  process.stdout.write(JSON.stringify(result));
}).catch(() => process.exit(7));
"""


def validate_producer(node, npm, cacache, node_version, npm_version, cacache_version):
    """Refuse if the on-disk producer is not the exact reviewed version."""
    if node_version != "26.7.0":
        raise RuntimeError(f"probe requires node 26.7.0; found {node_version!r}")
    if npm_version != "11.19.0":
        raise RuntimeError(f"probe requires npm 11.19.0; found {npm_version!r}")
    if cacache_version != "20.0.4":
        raise RuntimeError(f"probe requires cacache 20.0.4; found {cacache_version!r}")
    for name, path in (
        ("node", node),
        ("npm-cli", npm),
        ("cacache-entry", cacache / "lib" / "index.js"),
        ("cacache-verify", cacache / "lib" / "verify.js"),
    ):
        if not path.is_file():
            raise RuntimeError(f"{name} entry is not a regular file: {path}")


def git_wrapper(root, real_git):
    """Write a PATH-resolved git that pauses after a successful clone.

    This changes scheduling only. The real `git` binary, the npm distribution,
    and cacache are unmodified: the wrapper performs the real clone and only
    holds its *successful return* until the probe writes the resume marker.
    """
    bindir = root / "bin"
    bindir.mkdir()
    wrapper = bindir / "git"
    wrapper.write_text(
        # A /bin/sh wrapper avoids any interpreter-shebang/path-with-space issue.
        "#!/bin/sh\n"
        f'REAL_GIT={shlex.quote(str(real_git))}\n'
        # pacote prefixes git argv with `--no-replace-objects` and appends
        # `--recurse-submodules`; locate the `clone` subcommand anywhere in argv
        # and treat the following path argument that lives under a `tmp`
        # directory as the withTmp target the probe is stalling.
        'is_clone=0\n'
        'target=""\n'
        'for arg in "$@"; do\n'
        '  if [ "$arg" = "clone" ]; then is_clone=1; continue; fi\n'
        '  case "$arg" in\n'
        # The pacote withTmp target is always `<cache>/tmp/git-cloneXXXXXX`.
        '    */tmp/git-clone*) [ -n "$target" ] || target="$arg" ;;\n'
        '  esac\n'
        'done\n'
        'if [ "$is_clone" = "1" ]; then\n'
        '  "$REAL_GIT" "$@"\n'
        "  rc=$?\n"
        '  if [ "$rc" -eq 0 ]; then\n'
        '    printf %s "$target" > "$PROBE_CLONE_READY"\n'
        '    i=0\n'
        '    while [ ! -e "$PROBE_RESUME" ]; do\n'
        '      i=$((i+1))\n'
        '      if [ "$i" -gt 3000 ]; then exit 99; fi\n'
        '      sleep 0.02\n'
        "    done\n"
        "  fi\n"
        '  exit "$rc"\n'
        "fi\n"
        'exec "$REAL_GIT" "$@"\n'
    )
    wrapper.chmod(0o700)
    return bindir


def make_env(lab, bindir):
    return {
        "PATH": f"{bindir}:/usr/bin:/bin",
        "HOME": str(lab / "home"),
        "XDG_CACHE_HOME": str(lab / "cache"),
        "XDG_DATA_HOME": str(lab / "data"),
        "XDG_CONFIG_HOME": str(lab / "config"),
        # Isolate npm's own user config so no ambient config is read.
        "NPM_CONFIG_USERCONFIG": str(lab / "home" / ".npmrc"),
        "NPM_CONFIG_CACHE": str(lab / "npm-cache"),
        "NPM_CONFIG_OFFLINE": "true",
        "NPM_CONFIG_UPDATE_NOTIFIER": "false",
        "NPM_CONFIG_FUND": "false",
        "NPM_CONFIG_AUDIT": "false",
        "CI": "true",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_ASKPASS": "echo",
        "PROBE_CLONE_READY": str(lab / "clone-ready"),
        "PROBE_RESUME": str(lab / "resume"),
    }


def run_case(root, case, node, npm, real_git, bindir, cacache):
    lab = root / case
    for folder in ("home", "cache", "data", "config", "project", "source"):
        (lab / folder).mkdir(parents=True)
    env = make_env(lab, bindir)
    # `is_relative_to` is lexical; resolve macOS `/tmp` -> `/private/tmp` so the
    # withTmp target path (reported by git) and this root share a real prefix.
    # npm's `_cacache` lives directly under its configured cache directory.
    cache_root = (pathlib.Path(env["NPM_CONFIG_CACHE"]) / "_cacache").resolve()

    npm_version = subprocess.run(
        [node, npm, "--version"], cwd=lab, env=env,
        check=True, capture_output=True, text=True, timeout=30,
    ).stdout.strip()
    if npm_version != "11.19.0":
        raise RuntimeError(f"{case}: unexpected npm version {npm_version!r}")
    offline = subprocess.run(
        [node, npm, "config", "get", "offline"], cwd=lab, env=env,
        check=True, capture_output=True, text=True, timeout=30,
    ).stdout.strip()
    if offline != "true":
        raise RuntimeError(f"{case}: could not force offline; got {offline!r}")

    # A local git+file dependency: the only fetch is same-machine.
    source = lab / "source"
    (source / "package.json").write_text(json.dumps({
        "name": "local-git-fixture", "version": "1.0.0", "main": "index.js",
    }))
    (source / "index.js").write_text("module.exports = 42;\n")

    def source_git(*args):
        return subprocess.run(
            [str(real_git), *args], cwd=source, env=env, check=True,
            capture_output=True, text=True, timeout=30,
        ).stdout.strip()

    source_git("init", "-q")
    source_git("add", "package.json", "index.js")
    source_git(
        "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
        "-c", "commit.gpgsign=false", "commit", "-qm", "add local fixture",
    )
    commit = source_git("rev-parse", "HEAD")
    (lab / "project/package.json").write_text(json.dumps({
        "name": "local-race-consumer", "private": True,
        "dependencies": {"local-git-fixture": f"git+{source.as_uri()}#{commit}"},
    }))

    result = {"case": case}
    with (lab / "install.log").open("w") as log:
        child = subprocess.Popen(
            [node, npm, "install", "--ignore-scripts", "--no-audit", "--no-fund",
             "--loglevel=error", "--fetch-retries=0"],
            cwd=lab / "project", env=env, stdout=log, stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 60
            ready = lab / "clone-ready"
            while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            if not ready.exists():
                raise RuntimeError(f"{case}: git clone barrier was not reached")

            active_path = pathlib.Path(ready.read_text())
            active_before = (
                active_path.exists()
                and active_path.is_relative_to(cache_root)
                and (active_path / "package.json").is_file()
                and child.poll() is None
            )
            result["active_temp_before"] = active_before
            result["active_under_cache"] = active_path.is_relative_to(cache_root)
            result["active_in_tmp_bucket"] = active_path.is_relative_to(cache_root / "tmp")
            if not active_before:
                raise RuntimeError("fixture not paused with a populated live temp tree")

            if case == "interference":
                # Resolve the exact bundled package explicitly, never NODE_PATH.
                verify = subprocess.run(
                    [node, "-e", NATIVE_VERIFY, "--", str(cacache), str(cache_root)],
                    cwd=lab, env=env, capture_output=True, text=True, timeout=60,
                )
                (lab / "maintenance.log").write_text(verify.stdout + verify.stderr)
                result["maintenance_rc"] = verify.returncode
                try:
                    result["maintenance_counters"] = json.loads(verify.stdout)
                except ValueError:
                    result["maintenance_counters"] = None
                if child.poll() is not None:
                    raise RuntimeError("writer exited before the controlled interleaving")

            result["active_temp_after"] = active_path.exists()
            (lab / "resume").touch()
            result["install_rc"] = child.wait(timeout=60)
            installed = lab / "project/node_modules/local-git-fixture/index.js"
            result["installed_package"] = (
                installed.is_file() and installed.read_text() == "module.exports = 42;\n"
            )
        finally:
            # Release the git wrapper before terminating only our own child.
            (lab / "resume").touch()
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()

    result["install_log_tail"] = (lab / "install.log").read_text()[-4000:]
    return result


def classify(results):
    control, interference = results
    control_ok = (
        control["install_rc"] == 0
        and control["active_temp_before"]
        and control["active_temp_after"]
        and control["installed_package"]
    )
    if not control_ok:
        return "inconclusive_control", False
    interference_reproduced = (
        interference["maintenance_rc"] == 0
        and interference["active_temp_before"]
        and interference["active_in_tmp_bucket"]
        and interference["maintenance_counters"] == {"reclaimedCount": 0, "reclaimedSize": 0, "badContentCount": 0}
        and "ENOENT" in interference["install_log_tail"]
        and not interference["active_temp_after"]
        and not interference["installed_package"]
        and interference["install_rc"] != 0
    )
    if interference_reproduced:
        return "reproduced_deterministic_blocker", True
    interference_survived = (
        interference["maintenance_rc"] == 0
        and interference["active_temp_before"]
        and interference["active_temp_after"]
        and interference["installed_package"]
        and interference["install_rc"] == 0
    )
    if interference_survived:
        return "hypothesis_not_reproduced", False
    return "inconclusive_interference", False


def probe(root, node, npm, real_git, cacache):
    bindir = git_wrapper(root, real_git)
    results = [
        run_case(root, case, node, npm, real_git, bindir, cacache)
        for case in ("control", "interference")
    ]
    (root / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    verdict, reproduced = classify(results)
    print(json.dumps({"verdict": verdict, "results": results}, indent=2), flush=True)
    if reproduced:
        print(
            "\nNEGATIVE-ADMISSION EVIDENCE: npm 11.19.0 / cacache 20.0.4 native\n"
            "`verify()` cleanTmp removes the writer's live `_cacache/tmp` tree and\n"
            "the concurrent npm install fails. npm cache maintenance is NOT\n"
            "admitted for automatic runtime cleanup by this probe.",
            flush=True,
        )
    else:
        print(
            f"\nVerdict: {verdict}. This probe did not reproduce the unsafe\n"
            "interleaving; treat the hypothesis as unproven, not as admission.",
            flush=True,
        )
    return reproduced


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--node", type=pathlib.Path, required=True, help="exact node binary")
    parser.add_argument("--npm", type=pathlib.Path, required=True, help="npm JavaScript CLI entry")
    parser.add_argument("--cacache", type=pathlib.Path, required=True, help="cacache package directory")
    parser.add_argument("--git", type=pathlib.Path, required=True, help="real git binary")
    parser.add_argument("--evidence-dir", type=pathlib.Path,
                        help="retain fixtures in a new directory (must not already exist)")
    args = parser.parse_args()

    node = args.node.resolve(strict=True)
    npm = args.npm.resolve(strict=True)
    cacache = args.cacache.resolve(strict=True)
    real_git = args.git.resolve(strict=True)
    if npm.parent.parent != cacache.parent.parent:
        raise RuntimeError("npm CLI and cacache must belong to the same installation")
    node_version_run = subprocess.run(
        [str(node), "--version"], env={"PATH": "/usr/bin:/bin"},
        check=True, capture_output=True, text=True, timeout=30,
    )
    node_version = node_version_run.stdout.strip().lstrip("v")
    npm_version = json.loads((cacache.parent.parent / "package.json").read_text())["version"]
    cacache_version = json.loads((cacache / "package.json").read_text())["version"]
    validate_producer(node, npm, cacache, node_version, npm_version, cacache_version)


    if args.evidence_dir:
        # Refuse to reuse or overwrite an existing directory.
        args.evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
        reproduced = probe(args.evidence_dir.resolve(), node, npm, real_git, cacache)
    else:
        with tempfile.TemporaryDirectory(prefix="unlinger-npm-cache-probe-") as directory:
            reproduced = probe(pathlib.Path(directory).resolve(), node, npm, real_git, cacache)
    return 0 if reproduced else 1


if __name__ == "__main__":
    sys.exit(main())
