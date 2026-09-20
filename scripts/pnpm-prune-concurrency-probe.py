#!/usr/bin/env python3
"""Reproduce the pnpm 11.21.0 admission blocker using only disposable fixtures.

Exit zero means the expected unsafe interleaving was reproduced, not that pnpm
was admitted for automatic maintenance. No user store or installed service is used.
"""

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile
import time


def run_case(root, case, node, pnpm, git):
    lab = root / case
    for folder in ("home", "cache", "data", "config", "store", "project", "source"):
        (lab / folder).mkdir(parents=True)
    env = {
        "PATH": f"{root / 'bin'}:/usr/bin:/bin",
        "HOME": str(lab / "home"),
        "XDG_CACHE_HOME": str(lab / "cache"),
        "XDG_DATA_HOME": str(lab / "data"),
        "XDG_CONFIG_HOME": str(lab / "config"),
        "PNPM_HOME": str(lab / "data/pnpm"),
        "CI": "true",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_TERMINAL_PROMPT": "0",
        "LAB_CLONE_READY": str(lab / "clone-ready"),
        "LAB_RESUME": str(lab / "resume"),
    }
    version = subprocess.run(
        [node, pnpm, "--version"], cwd=lab, env=env,
        check=True, capture_output=True, text=True, timeout=15,
    ).stdout.strip()
    if version != "11.21.0":
        raise RuntimeError(f"this probe requires pnpm 11.21.0; found {version}")

    source = lab / "source"
    (source / "package.json").write_text(json.dumps({
        "name": "local-git-fixture", "version": "1.0.0", "main": "index.js",
    }))
    (source / "index.js").write_text("module.exports = 42;\n")

    def source_git(*args):
        return subprocess.run(
            [git, *args], cwd=source, env=env, check=True,
            capture_output=True, text=True, timeout=15,
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
    common = [
        "--store-dir", str(lab / "store"),
        f"--config.cache-dir={lab / 'cache'}", "--config.offline=true",
    ]
    with (lab / "install.log").open("w") as log:
        child = subprocess.Popen(
            [node, pnpm, "install", *common, "--ignore-scripts", "--reporter=append-only"],
            cwd=lab / "project", env=env, stdout=log, stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 35
            ready = lab / "clone-ready"
            while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            if not ready.exists():
                raise RuntimeError(f"{case}: Git clone barrier was not reached")
            active_path = pathlib.Path(ready.read_text())
            if not active_path.is_relative_to(lab / "store"):
                raise RuntimeError("clone escaped the test-owned store")
            if not (active_path / "package.json").is_file() or child.poll() is not None:
                raise RuntimeError("fixture was not paused with a populated live temporary tree")

            prune_rc = None
            if case == "prune_during_clone":
                prune = subprocess.run(
                    [node, pnpm, "store", "prune", *common], cwd=lab, env=env,
                    capture_output=True, text=True, timeout=20,
                )
                (lab / "prune.log").write_text(prune.stdout + prune.stderr)
                prune_rc = prune.returncode
                if child.poll() is not None:
                    raise RuntimeError("install exited before the controlled interleaving")

            retained = active_path.exists()
            (lab / "resume").touch()
            install_rc = child.wait(timeout=20)
            installed = lab / "project/node_modules/local-git-fixture/index.js"
            result = {
                "case": case,
                "active_temp_before": True,
                "active_temp_after": retained,
                "prune_rc": prune_rc,
                "install_rc": install_rc,
                "installed_package": installed.is_file() and installed.read_text() == "module.exports = 42;\n",
            }
        finally:
            # Release our Git wrapper before terminating only the retained pnpm
            # child. The wrapper also has its own finite deadline.
            (lab / "resume").touch()
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()

    log_text = (lab / "install.log").read_text()
    if case == "control":
        expected = install_rc == 0 and retained and result["installed_package"]
    else:
        expected = (
            prune_rc == 0 and not retained and install_rc != 0
            and not result["installed_package"]
            and "ENOENT" in log_text and "cwd" in log_text
        )
    if not expected:
        raise RuntimeError(f"unexpected probe result: {json.dumps(result)}\n{log_text}")
    print(json.dumps(result), flush=True)
    return result


def probe(root, node, pnpm, git):
    (root / "bin").mkdir()
    wrapper = root / "bin/git"
    # This wrapper changes scheduling only: after a real local clone, hold its
    # successful return until prune finishes. The pnpm distribution is unmodified.
    wrapper.write_text(f"#!{sys.executable}\n" + f"REAL_GIT = {str(git)!r}\n" + """
import os, pathlib, subprocess, sys, time
result = subprocess.run([REAL_GIT, *sys.argv[1:]])
if result.returncode == 0 and sys.argv[1:2] == ['clone']:
    pathlib.Path(os.environ['LAB_CLONE_READY']).write_text(sys.argv[-1])
    deadline = time.monotonic() + 45
    while not pathlib.Path(os.environ['LAB_RESUME']).exists():
        if time.monotonic() > deadline:
            sys.exit(99)
        time.sleep(.02)
sys.exit(result.returncode)
""")
    wrapper.chmod(0o700)
    results = [run_case(root, case, node, pnpm, git) for case in ("control", "prune_during_clone")]
    (root / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print("Expected admission blocker reproduced; pnpm remains unsupported for automatic maintenance.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--node", type=pathlib.Path, required=True)
    parser.add_argument("--pnpm", type=pathlib.Path, required=True, help="pnpm JavaScript CLI entry")
    parser.add_argument("--git", type=pathlib.Path, default=pathlib.Path("/usr/bin/git"))
    parser.add_argument("--evidence-dir", type=pathlib.Path, help="retain fixtures in a new directory")
    args = parser.parse_args()
    node, pnpm, git = (path.resolve(strict=True) for path in (args.node, args.pnpm, args.git))
    if args.evidence_dir:
        args.evidence_dir.mkdir(mode=0o700)  # Refuse to reuse or overwrite an existing directory.
        probe(args.evidence_dir.resolve(), node, pnpm, git)
    else:
        with tempfile.TemporaryDirectory(prefix="unlinger-pnpm-prune-probe-") as directory:
            probe(pathlib.Path(directory).resolve(), node, pnpm, git)


if __name__ == "__main__":
    main()
