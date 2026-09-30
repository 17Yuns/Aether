"""Exercise the actual release workflow's tag classification script."""

import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest


WORKFLOW = Path(__file__).resolve().parents[1] / ".github/workflows/release.yml"
SHA = "1234567890abcdef1234567890abcdef1234567890"


def preflight_script():
    source = WORKFLOW.read_text(encoding="utf-8")
    step = source.index("      - name: Classify release tag\n")
    start = source.index("        run: |\n", step) + len("        run: |\n")
    lines = []
    for line in source[start:].splitlines(keepends=True):
        if line.strip() and not line.startswith("          "):
            break
        lines.append(line)
    return textwrap.dedent("".join(lines))


class ReleasePreflightTests(unittest.TestCase):
    def classify(self, event="workflow_dispatch", ref_type="branch", ref_name="main", version=""):
        with tempfile.TemporaryDirectory(prefix="aether-release-preflight-") as directory:
            output = Path(directory) / "output"
            environment = {
                **os.environ,
                "GITHUB_EVENT_NAME": event,
                "GITHUB_REF_TYPE": ref_type,
                "GITHUB_REF_NAME": ref_name,
                "GITHUB_SHA": SHA,
                "GITHUB_OUTPUT": output.as_posix(),
                "INPUT_VERSION_TAG": version,
            }
            result = subprocess.run(
                [os.environ.get("AETHER_TEST_BASH", "bash"), "-c", preflight_script()],
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            values = dict(
                line.split("=", 1)
                for line in output.read_text().splitlines()
            ) if output.exists() else {}
            return result, values

    def test_manual_branch_run_publishes_snapshot(self):
        result, values = self.classify()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(values, {
            "publish": "true",
            "version_tag": "snapshot-1234567",
            "prerelease": "true",
            "make_latest": "false",
        })

    def test_versioned_runs_publish_with_the_correct_release_type(self):
        cases = [
            ({"version": "v1.2.3"}, "v1.2.3", "false", "true"),
            ({"version": "v1.2.3-beta.1"}, "v1.2.3-beta.1", "true", "false"),
            ({"version": "v1.2.3-rc.2"}, "v1.2.3-rc.2", "true", "false"),
            ({"event": "push", "ref_type": "tag", "ref_name": "v2.0.0"}, "v2.0.0", "false", "true"),
            ({"ref_type": "tag", "ref_name": "v2.1.0"}, "v2.1.0", "false", "true"),
        ]
        for arguments, tag, prerelease, latest in cases:
            with self.subTest(arguments=arguments):
                result, values = self.classify(**arguments)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(values["publish"], "true")
                self.assertEqual(values["version_tag"], tag)
                self.assertEqual(values["prerelease"], prerelease)
                self.assertEqual(values["make_latest"], latest)

    def test_invalid_versions_cannot_publish(self):
        for version in ["main", "v1.2", "v1.2.3; echo unexpected", "v1.2.3\npublish=true"]:
            with self.subTest(version=version):
                result, values = self.classify(version=version)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotEqual(values.get("publish"), "true")


if __name__ == "__main__":
    unittest.main()
