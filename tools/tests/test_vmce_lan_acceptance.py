"""Unit tests for tools.vmce_lan_acceptance.

These are pure comparator/validation/settings tests.  They launch no game and
probe no sockets, so they run in the ordinary Python suite.

python -m unittest tools.tests.test_vmce_lan_acceptance -v
"""

from __future__ import annotations

import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest

from tools import vmce_lan_acceptance as acceptance


def synthetic_receipt() -> dict:
    """A minimal receipt using the real VMCE live-capture field paths."""
    return {
        "checkpoint": "lan-live-vera-campaign-host",
        "lan": {
            "bind": "127.0.0.1:32100",
            "checksum_failure": "None",
            "failure": None,
            "native_checksums": list(range(300)),
            "simulation_hashes": [[15, 111], [30, 222], [45, 333]],
            "startup": {
                "map": "VMCE/LDA-Continue/stage01/all01umd.map",
                "seed": 4242,
            },
            "vera_outcome": {
                "kind": "real",
                "native_frame": 630,
                "dialog": {
                    "victory": True,
                    "rows": 2,
                    "can_continue": True,
                    "render_frame": 823,
                    "time": "00:00:23",
                },
                "next_mission": {
                    "map": "VMCE/LDA-Continue/stage02/all01umd.map",
                    "native_frame": 60,
                    "state_hash": 999,
                    "render_frame": 981,
                },
                "state_hash": 123456789,
            },
            "vmce": {
                "host": True,
                "map_file": "VMCE/LDA-Continue/stage01/all01umd.map",
                "series_sha256": "a" * 64,
            },
        },
        "screenshots": [
            {"name": "vera-result-real-win", "file": "shot-01.png", "byte_length": 1},
            {"name": "vera-next-mission", "file": "shot-02.png", "byte_length": 1},
        ],
    }


def two_receipts() -> list[tuple[str, dict]]:
    host = synthetic_receipt()
    guest = copy.deepcopy(host)
    guest["checkpoint"] = "lan-live-vera-campaign-guest"
    guest["lan"]["bind"] = "127.0.0.1:32110"
    guest["lan"]["vmce"]["host"] = False
    return [("host", host), ("guest1", guest)]


def failed(checks: list[acceptance.Check]) -> list[acceptance.Check]:
    return [check for check in checks if not check.ok]


def assert_all_pass(test: unittest.TestCase, checks: list[acceptance.Check]) -> None:
    test.assertTrue(
        all(check.ok for check in checks),
        [(check.name, check.detail) for check in checks if not check.ok],
    )


class ComparatorTests(unittest.TestCase):
    def evaluate(self, receipts, **overrides):
        options = {
            "players": 2,
            "expect_victory": True,
            "expect_can_continue": True,
            "expect_rows": 2,
            "expect_next_stage": True,
        }
        options.update(overrides)
        return acceptance.evaluate_receipts(receipts, **options)

    def test_equal_two_receipts_pass(self):
        checks = self.evaluate(two_receipts())
        assert_all_pass(self, checks)
        self.assertEqual(failed(checks), [])

    def test_checksum_mismatch_reports_index_path_and_labels(self):
        receipts = two_receipts()
        receipts[1][1]["lan"]["native_checksums"][57] = 999_999
        checks = self.evaluate(receipts)
        mismatch = [check for check in failed(checks) if check.name == "native_checksums equal"]
        self.assertEqual(len(mismatch), 1)
        self.assertIn("lan.native_checksums[57]", mismatch[0].detail)
        self.assertIn("host=", mismatch[0].detail)
        self.assertIn("guest1=", mismatch[0].detail)

    def test_rows_mismatch_fails(self):
        receipts = two_receipts()
        receipts[1][1]["lan"]["vera_outcome"]["dialog"]["rows"] = 3
        checks = self.evaluate(receipts)
        rows = [check for check in failed(checks) if check.name == "dialog.rows"]
        self.assertEqual(len(rows), 1)
        self.assertIn("lan.vera_outcome.dialog.rows", rows[0].detail)

    def test_missing_screenshot_fails(self):
        receipts = two_receipts()
        receipts[1][1]["screenshots"] = []
        checks = self.evaluate(receipts)
        self.assertEqual(
            [check.name for check in failed(checks)], ["screenshots"]
        )

    def test_missing_simulation_hashes_needs_allow_flag(self):
        receipts = two_receipts()
        receipts[1][1]["lan"]["simulation_hashes"] = []

        strict = self.evaluate(receipts)
        self.assertIn("simulation_hashes", [check.name for check in failed(strict)])

        allowed = self.evaluate(receipts, allow_missing_simulation_hashes=True)
        assert_all_pass(self, allowed)

    def test_missing_state_hash_reports_the_real_path(self):
        receipts = two_receipts()
        del receipts[1][1]["lan"]["vera_outcome"]["state_hash"]
        checks = self.evaluate(receipts)
        state = [check for check in failed(checks) if check.name == "state_hash"]
        self.assertEqual(len(state), 1)
        self.assertIn("lan.vera_outcome.state_hash", state[0].detail)

    def test_expect_next_stage_requires_a_map(self):
        receipts = two_receipts()
        del receipts[1][1]["lan"]["vera_outcome"]["next_mission"]
        checks = self.evaluate(receipts)
        next_stage = [check for check in failed(checks) if check.name == "next_mission.map"]
        self.assertEqual(len(next_stage), 1)
        self.assertIn("lan.vera_outcome.next_mission.map", next_stage[0].detail)

    def test_expected_dialog_value_is_asserted(self):
        receipts = two_receipts()
        for _, receipt in receipts:
            receipt["lan"]["vera_outcome"]["dialog"]["victory"] = False
        checks = self.evaluate(receipts)
        victory = [check for check in failed(checks) if check.name == "dialog.victory"]
        self.assertEqual(len(victory), 1)
        self.assertIn("expected true", victory[0].detail)


class CompareCliTests(unittest.TestCase):
    def test_compare_main_returns_zero_then_one(self):
        receipts = two_receipts()
        with tempfile.TemporaryDirectory() as base:
            base = Path(base)
            host_path = base / "host.json"
            guest_path = base / "guest.json"
            host_path.write_text(json.dumps(receipts[0][1]), encoding="utf-8")
            guest_path.write_text(json.dumps(receipts[1][1]), encoding="utf-8")
            arguments = [
                "compare",
                "--players",
                "2",
                "--receipt",
                str(host_path),
                "--receipt",
                str(guest_path),
                "--expect-victory",
                "--expect-can-continue",
                "--expect-rows",
                "2",
            ]
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                status = acceptance.main(arguments)
            self.assertEqual(status, 0, output.getvalue())
            self.assertIn("RESULT: PASS", output.getvalue())

            receipts[1][1]["lan"]["vera_outcome"]["dialog"]["rows"] = 3
            guest_path.write_text(json.dumps(receipts[1][1]), encoding="utf-8")
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                status = acceptance.main(arguments)
            self.assertEqual(status, 1, output.getvalue())
            self.assertIn("RESULT: FAIL", output.getvalue())


class ValidationTests(unittest.TestCase):
    def test_players(self):
        self.assertEqual(acceptance.validate_players(2), 2)
        self.assertEqual(acceptance.validate_players(4), 4)
        for value in (0, 1, 3, 5, "2", True, None):
            with self.subTest(value=value), self.assertRaises(acceptance.UsageError):
                acceptance.validate_players(value)

    def test_timeout(self):
        self.assertEqual(acceptance.validate_timeout_seconds(1), 1.0)
        self.assertEqual(acceptance.validate_timeout_seconds(600), 600.0)
        for value in (0, -1, float("nan"), float("inf"), "600", True, None):
            with self.subTest(value=value), self.assertRaises(acceptance.UsageError):
                acceptance.validate_timeout_seconds(value)

    def test_peer_counts(self):
        acceptance.validate_peer_count(4, "host", ["a", "b", "c"])
        acceptance.validate_peer_count(4, "guest", ["a"])
        acceptance.validate_peer_count(2, "host", ["a"])
        with self.assertRaises(acceptance.UsageError):
            acceptance.validate_peer_count(4, "host", ["a"])
        with self.assertRaises(acceptance.UsageError):
            acceptance.validate_peer_count(4, "guest", ["a", "b"])
        with self.assertRaises(acceptance.UsageError):
            acceptance.validate_peer_count(2, "host", ["a", "b"])

    def test_port_base_and_process_ports(self):
        self.assertEqual(acceptance.validate_port_base(32100, 4), 32100)
        self.assertEqual(acceptance.process_ports(32100, 4), (32100, 32110, 32120, 32130))
        self.assertEqual(acceptance.process_ports(32100, 2), (32100, 32110))
        with self.assertRaises(acceptance.UsageError):
            acceptance.validate_port_base(0, 4)
        with self.assertRaises(acceptance.UsageError):
            acceptance.validate_port_base(65530, 4)

    def test_jobs_rejects_parallel_scenarios(self):
        self.assertEqual(acceptance.validate_jobs(1), 1)
        for value in (0, -1, True, "1"):
            with self.subTest(value=value), self.assertRaises(acceptance.UsageError):
                acceptance.validate_jobs(value)
        with self.assertRaisesRegex(acceptance.UsageError, "scenario parallelism not enabled"):
            acceptance.validate_jobs(2)


class SettingsTemplateTests(unittest.TestCase):
    def test_config_marker_and_series_copy(self):
        with tempfile.TemporaryDirectory() as base:
            base = Path(base)
            retail = base / "retail"
            retail.mkdir()
            (retail / "ra2.mix").write_bytes(b"retail")
            series = base / "LDA-Continue"
            (series / "stage01").mkdir(parents=True)
            (series / "campaign.ini").write_text("[Campaign]\n", encoding="utf-8")
            (series / "stage01" / "all01umd.map").write_bytes(b"map")
            settings = base / "settings"

            config_path = acceptance.write_process_settings(settings, retail, series)

            config = config_path.read_text(encoding="utf-8")
            expected_ra2 = retail.resolve().as_posix()
            self.assertIn(f'ra2_dir = "{expected_ra2}"', config)
            self.assertIn('settings_dir = "."', config)
            self.assertNotIn("\\", config)
            self.assertEqual(
                (settings / acceptance.CAPTURE_ROOT_MARKER).read_bytes(),
                b"VERA20k isolated shell capture fixture\n",
            )
            copied = settings / "Maps" / "VMCE" / "LDA-Continue"
            self.assertTrue((copied / "campaign.ini").is_file())
            self.assertTrue((copied / "stage01" / "all01umd.map").is_file())
            self.assertEqual(
                acceptance.build_lan_map_name(series, "stage01/all01umd.map"),
                "VMCE/LDA-Continue/stage01/all01umd.map",
            )


class CaptureCommandTests(unittest.TestCase):
    def test_four_player_host_passes_players_peers_map_and_name(self):
        command = acceptance.build_capture_command(
            exe=Path("vera20k"),
            role="host",
            players=4,
            bind="127.0.0.1:32100",
            peers=("127.0.0.1:32110", "127.0.0.1:32120", "127.0.0.1:32130"),
            output=Path("out"),
            map_name="VMCE/LDA-Continue/stage01/all01umd.map",
            lobby_name="LiveHost",
        )
        self.assertEqual(command[0], "vera20k")
        self.assertIn("--shell-capture", command)
        self.assertEqual(command[command.index("--shell-capture") + 1],
                         "lan-live-vera-campaign-host")
        self.assertEqual(command[command.index("--lan-players") + 1], "4")
        self.assertEqual(
            command[command.index("--lan-peer") + 1],
            "127.0.0.1:32110,127.0.0.1:32120,127.0.0.1:32130",
        )
        self.assertEqual(
            command[command.index("--lan-map") + 1],
            "VMCE/LDA-Continue/stage01/all01umd.map",
        )
        self.assertEqual(command[command.index("--lan-name") + 1], "LiveHost")

    def test_two_player_guest_has_host_peer_and_no_map_or_name(self):
        command = acceptance.build_capture_command(
            exe=Path("vera20k"),
            role="guest",
            players=2,
            bind="127.0.0.1:32110",
            peers=("127.0.0.1:32100",),
            output=Path("out"),
            map_name=None,
            lobby_name=None,
        )
        self.assertEqual(command[command.index("--shell-capture") + 1],
                         "lan-live-vera-campaign-guest")
        self.assertEqual(command[command.index("--lan-peer") + 1], "127.0.0.1:32100")
        self.assertNotIn("--lan-map", command)
        self.assertNotIn("--lan-name", command)


class WrapperHelperTests(unittest.TestCase):
    def test_kill_tree_plan_selects_taskkill_on_windows(self):
        plan = acceptance.kill_tree_plan(4321, "win32")
        self.assertEqual(plan.kind, "taskkill")
        self.assertEqual(plan.argv, ("taskkill", "/PID", "4321", "/T", "/F"))
        self.assertEqual(acceptance.kill_tree_plan(4321, "linux").kind, "killpg")
        self.assertEqual(acceptance.kill_tree_plan(4321, "darwin").kind, "killpg")

    def test_timeout_classification(self):
        self.assertTrue(acceptance.is_timeout(5.0, 5.0))
        self.assertTrue(acceptance.is_timeout(5.1, 5.0))
        self.assertFalse(acceptance.is_timeout(4.9, 5.0))

    def test_room_marker_detection(self):
        self.assertTrue(acceptance.host_room_marker_seen("live LAN LiveVmceHost render 12"))
        self.assertFalse(acceptance.host_room_marker_seen("loading config"))


if __name__ == "__main__":
    unittest.main()
