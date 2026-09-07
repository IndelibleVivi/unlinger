"""Real child-process regression for demo teardown and isolated App state."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("demo_window", Path(__file__).with_name("demo-window.py"))
demo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(demo)


class DemoOwnershipTests(unittest.TestCase):
    def test_app_exit_cleans_only_owned_daemon_and_temporary_state(self):
        with tempfile.TemporaryDirectory() as stage:
            root = Path(stage)
            record = root / "record.json"
            daemon = root / "daemon.py"
            daemon.write_text("import socket,sys,time\ns=socket.socket(socket.AF_UNIX)\n"
                              "s.bind(sys.argv[sys.argv.index('--socket')+1])\ntime.sleep(120)\n")
            app = root / "app.py"
            app.write_text("import json,os,sys\nfrom pathlib import Path\n"
                           "Path(sys.argv[1]).write_text(json.dumps({k:os.environ[k] for k in "
                           "['UNLINGER_SOCKET_PATH','CFFIXED_USER_HOME']}))\n")
            # A separate same-executable process must survive demo teardown.
            control = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(120)"])
            try:
                self.assertEqual(demo.run_demo([sys.executable, str(daemon)],
                                               [sys.executable, str(app), str(record)]), 0)
                values = json.loads(record.read_text())
                state = Path(values['UNLINGER_SOCKET_PATH']).parent
                self.assertEqual(Path(values['CFFIXED_USER_HOME']), state / 'home')
                self.assertFalse(state.exists())
                self.assertIsNone(control.poll())
            finally:
                demo.stop_child(control)

    def test_daemon_start_failure_is_reported(self):
        with self.assertRaisesRegex(RuntimeError, 'did not become available'):
            demo.run_demo([sys.executable, '-c', 'raise SystemExit(3)'], ['/usr/bin/true'])


if __name__ == '__main__':
    unittest.main()
