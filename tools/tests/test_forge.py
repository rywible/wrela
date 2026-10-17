"""forge's command (tools/forge): the service host's command line for each way it runs."""
from importlib.machinery import SourceFileLoader
from pathlib import Path
import tempfile
import unittest

forge = SourceFileLoader("forge_command", str(Path(__file__).resolve().parents[1] / "forge")).load_module()


class Command(unittest.TestCase):
    def test_a_call_runs_the_pinned_install_with_this_directory_as_its_workspace(self):
        with tempfile.TemporaryDirectory() as home, tempfile.TemporaryDirectory() as here:
            line = forge.command(["pull", "change=c3"], here, Path(home))
            self.assertEqual(line[0], str(Path(home) / "current/wrela-service"))
            self.assertEqual(line[1], str(Path(home) / "current/build"))
            self.assertEqual(line[2:4], ["--root", f"here={Path(here).resolve()}"])
            self.assertEqual(line[4:], ["--", "pull", "change=c3"])

    def test_the_service_keeps_its_store_and_checkout_where_it_is_told(self):
        with tempfile.TemporaryDirectory() as home, tempfile.TemporaryDirectory() as store:
            line = forge.command(["serve", "--data", store, "--listen", "0.0.0.0:7171"], "/", Path(home))
            self.assertIn(f"data={Path(store).resolve()}", line)
            self.assertIn(f"work={Path(home) / 'work'}", line)
            self.assertEqual(line[line.index("--"):], ["--", "serve", "--listen", "0.0.0.0:7171"])
            self.assertTrue((Path(home) / "work").is_dir())


if __name__ == "__main__":
    unittest.main()
