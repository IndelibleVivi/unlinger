#!/usr/bin/env python3
"""Own one foreground demo's children; never rediscover processes by name/PID file."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def stop_child(child):
    # Popen retains an unreaped child identity; it checks exit before signalling.
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=30)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def run_demo(daemon_command, app_command):
    children = []
    with tempfile.TemporaryDirectory(prefix="unlinger-window-") as stage:
        root = Path(stage).resolve()
        home = root / "home"
        home.mkdir(mode=0o700)
        endpoint = root / "unlingerd.sock"
        with (root / "daemon.log").open("w+") as log:
            try:
                daemon = subprocess.Popen(
                    [*daemon_command, "--report-only", "--database", str(root / "history.sqlite3"),
                     "--socket", str(endpoint), "--instance-lock", str(root / "instance.lock")],
                    stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
                )
                children.append(daemon)
                deadline = time.monotonic() + 20
                while not endpoint.is_socket():
                    if daemon.poll() is not None or time.monotonic() >= deadline:
                        log.seek(0)
                        raise RuntimeError("demo daemon did not become available:\n" + log.read())
                    time.sleep(0.1)
                env = os.environ.copy()
                env.pop("UNLINGER_FIXTURE", None)
                env.pop("UNLINGER_FIXTURE_ROUTE", None)
                env.update(UNLINGER_SOCKET_PATH=str(endpoint), UNLINGER_WINDOW="1",
                           CFFIXED_USER_HOME=str(home))
                # Use the unbundled source executable: no packaged notifications.
                app = subprocess.Popen(app_command, env=env, start_new_session=True)
                children.append(app)
                print("Report-only demo. Quit its App or press Ctrl-C here to stop. Temporary state: "
                      + str(root), flush=True)
                while app.poll() is None:
                    if daemon.poll() is not None:
                        raise RuntimeError("demo daemon exited while the App was open")
                    time.sleep(0.1)
                return app.returncode
            finally:
                for child in reversed(children):
                    stop_child(child)


def main():
    def interrupted(_signal, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    try:
        return run_demo([sys.argv[1]], [sys.argv[2]])
    except KeyboardInterrupt:
        return 130
    except (OSError, RuntimeError) as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
