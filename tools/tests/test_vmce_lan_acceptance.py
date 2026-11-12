"""Unit tests for tools.vmce_lan_acceptance.

These are pure comparator/validation/settings tests.  They launch no game and
probe no sockets, so they run in the ordinary Python suite.

python -m unittest tools.tests.test_vmce_lan_acceptance -v
"""

from __future__ import annotations

import contextlib
import copy
import hashlib
import io
import json
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
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

    def test_final_requires_both_peers_to_return_to_the_menu(self):
        receipts = two_receipts()
        for _, receipt in receipts:
            receipt["lan"]["vera_outcome"] = {
                "kind": "end", "completed": True,
                "screen": "MainMenu", "network_return": "None",
            }
        assert_all_pass(self, self.evaluate(receipts, expect_final=True))
        receipts[1][1]["lan"]["vera_outcome"]["screen"] = "InGame"
        self.assertTrue(failed(self.evaluate(receipts, expect_final=True)))

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
        self.assertEqual(acceptance.validate_players(3), 3)
        self.assertEqual(acceptance.validate_players(4), 4)
        for value in (0, 1, 5, "2", True, None):
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


class ScenarioCheckpointGuardTests(unittest.TestCase):
    def test_vmce_scenarios_are_accepted(self) -> None:
        for name in ("2p-retry", "2p-win", "4p-win"):
            acceptance.validate_scenario_checkpoints(acceptance.SCENARIOS[name])

    def test_non_vmce_checkpoint_is_rejected_before_launch(self) -> None:
        scenario = acceptance.Scenario(
            name="2p-retry",
            players=2,
            expect_victory=False,
            expect_can_continue=True,
            expect_next_stage=False,
            checkpoint_host="lan-live-coop-host",
            checkpoint_guest="lan-live-coop-guest",
            expect_retry=True,
        )
        with self.assertRaisesRegex(acceptance.UsageError, "non-VMCE host checkpoint"):
            acceptance.validate_scenario_checkpoints(scenario)


class A03AcceptanceTests(unittest.TestCase):
    def make_receipts(self, marker_path: Path) -> list[tuple[str, dict]]:
        marker_sha = "f" * 64
        base = {
            "handoff_refused": True,
            "blocking_notice": "The campaign changed; Ready again.",
            "notice_acknowledged": True,
            "room_exit_observed": True,
            "rendezvous_marker_path": str(marker_path.resolve()),
            "rendezvous_marker_sha256": marker_sha,
            "rendezvous_marker_written": False,
            "rendezvous_marker_observed": False,
            "rendezvous_wait_frames": None,
            "rendezvous_timeout_frames": acceptance.A03_MARKER_TIMEOUT_FRAMES,
        }
        host = {
            "checkpoint": acceptance.CHECKPOINT_A03_HOST,
            "lan": {
                "route": [
                    {
                        "render_frame": 20,
                        "action": "A03 host observed the guest's admitted-Ready mutation marker; Start gate released",
                    },
                    {
                        "render_frame": 20,
                        "action": "host Start clicked; VMCE admission and frozen start",
                    },
                ],
                "a03": {
                    **base,
                    "role": "host",
                    "host_start_clicked": True,
                    "failure_phase": "loading",
                    "loading_observed": True,
                    "startup_failure_observed": True,
                    "gameplay_active_at_notice": False,
                    "locked_when_notice": False,
                    "rendezvous_marker_observed": True,
                    "rendezvous_wait_frames": 3,
                },
            },
            "screenshots": [{"name": "vera-a03-handoff-refused"}],
        }
        guest = {
            "checkpoint": acceptance.CHECKPOINT_A03_GUEST,
            "lan": {
                "a03": {
                    **base,
                    "role": "guest",
                    "failure_phase": "setup",
                    "loading_observed": False,
                    "startup_failure_observed": False,
                    "gameplay_active_at_notice": None,
                    "locked_when_notice": True,
                    "rendezvous_marker_written": True,
                    "ready_generation_before": 5,
                    "ready_generation_after": 6,
                    "mutation_after_admitted_ready": True,
                    "selected_map": "stage01/all01umd.map",
                    "campaign_sha256_before": "c" * 64,
                    "campaign_sha256_after": "c" * 64,
                    "selected_sha256_before": "d" * 64,
                    "selected_sha256_after": "d" * 64,
                    "unselected_later_map": "stage02/all01umd.map",
                    "unselected_sha256_before": "a" * 64,
                    "unselected_sha256_mutated": "b" * 64,
                    "unselected_sha256_restored": "a" * 64,
                }
            },
            "screenshots": [{"name": "vera-a03-handoff-refused"}],
        }
        return [("host", host), ("guest1", guest)]

    def test_a03_selection_adds_distinct_recovery_without_changing_defaults(self):
        ordinary = acceptance.selected_scenarios(
            SimpleNamespace(scenario=[], suite="fast")
        )
        self.assertEqual([scenario.name for scenario in ordinary], ["4p-win"])

        combined = acceptance.selected_scenarios(
            SimpleNamespace(
                scenario=["a03-stale-later-map", "2p-win"],
                suite="fast",
            )
        )
        self.assertEqual(
            [scenario.name for scenario in combined],
            ["a03-stale-later-map", "a03-recovery-2p-win", "2p-win"],
        )
        acceptance.validate_scenario_checkpoints(acceptance.SCENARIOS["a03-stale-later-map"])

    def test_a03_requires_admitted_ready_notice_exit_and_pre_start_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            marker_path = Path(directory) / "guest-mutation.json"
            receipts = self.make_receipts(marker_path)
            checks = acceptance._a03_refusal_receipt_checks(
                receipts,
                expected_entry="stage01/all01umd.map",
                series_name="LDA-Continue-real",
                marker_path=marker_path,
            )
            assert_all_pass(self, checks)

            failures_to_exercise = (
                ("admitted Ready", "guest1", "ready_generation_after", 5),
                ("manifest unchanged", "guest1", "campaign_sha256_after", "e" * 64),
                ("selected map unchanged", "guest1", "selected_sha256_after", "e" * 64),
                ("exact map restoration hash", "guest1", "unselected_sha256_restored", "e" * 64),
                ("host loading_observed", "host", "loading_observed", False),
                ("host startup_failure_observed", "host", "startup_failure_observed", False),
                ("host gameplay_active_at_notice", "host", "gameplay_active_at_notice", True),
                ("guest locked Go refusal", "guest1", "locked_when_notice", False),
                ("room_exit_observed guest1", "guest1", "room_exit_observed", False),
                ("host observed guest mutation", "host", "rendezvous_marker_observed", False),
                ("host marker wait", "host", "rendezvous_wait_frames", acceptance.A03_MARKER_TIMEOUT_FRAMES),
                ("guest published mutation marker", "guest1", "rendezvous_marker_written", False),
            )
            for expected_check, role, field, value in failures_to_exercise:
                with self.subTest(expected_check=expected_check):
                    bad = copy.deepcopy(receipts)
                    target = 0 if role == "host" else 1
                    bad[target][1]["lan"]["a03"][field] = value
                    checks = acceptance._a03_refusal_receipt_checks(
                        bad,
                        expected_entry="stage01/all01umd.map",
                        series_name="LDA-Continue-real",
                        marker_path=marker_path,
                    )
                    self.assertTrue(
                        any(
                            not check.ok and expected_check in check.name
                            for check in checks
                        ),
                        [(check.name, check.ok) for check in checks],
                    )

            with self.subTest(expected_check="host observed marker before Start"):
                bad = copy.deepcopy(receipts)
                bad[0][1]["lan"]["route"].reverse()
                checks = acceptance._a03_refusal_receipt_checks(
                    bad,
                    expected_entry="stage01/all01umd.map",
                    series_name="LDA-Continue-real",
                    marker_path=marker_path,
                )
                self.assertTrue(
                    any(
                        not check.ok and "host observed marker before Start" in check.name
                        for check in checks
                    )
                )

    def test_a03_selected_map_accepts_only_the_expected_series_prefix(self):
        with tempfile.TemporaryDirectory() as directory:
            marker_path = Path(directory) / "guest-mutation.json"
            receipts = self.make_receipts(marker_path)
            guest_a03 = receipts[1][1]["lan"]["a03"]
            guest_a03["selected_map"] = (
                "VMCE/LDA-Continue-real/stage01/all01umd.map"
            )

            checks = acceptance._a03_refusal_receipt_checks(
                receipts,
                expected_entry="stage01/all01umd.map",
                series_name="LDA-Continue-real",
                marker_path=marker_path,
            )
            selected_check = next(check for check in checks if check.name == "A03 selected entry")
            self.assertTrue(selected_check.ok, selected_check.detail)

            for selected in (
                "VMCE/another-series/stage01/all01umd.map",
                "elsewhere/stage01/all01umd.map",
                "../stage01/all01umd.map",
            ):
                with self.subTest(selected=selected):
                    guest_a03["selected_map"] = selected
                    checks = acceptance._a03_refusal_receipt_checks(
                        receipts,
                        expected_entry="stage01/all01umd.map",
                        series_name="LDA-Continue-real",
                        marker_path=marker_path,
                    )
                    selected_check = next(
                        check for check in checks if check.name == "A03 selected entry"
                    )
                    self.assertFalse(selected_check.ok, selected_check.detail)

    def test_a03_requires_the_original_admission_startup_failure_log(self):
        with tempfile.TemporaryDirectory() as directory:
            log_path = Path(directory) / "ra2.log"
            log_path.write_text(
                "Network connection destroyed: source=1, house=Some(9), reason=1, "
                "native_frame=0, startup_pending=false\n"
                "Could not load map: initial active roster is not a majority of the original admission\n"
                "Returned to the shell\n",
                encoding="utf-8",
            )
            assert_all_pass(self, acceptance._a03_host_startup_log_checks(log_path))

            for changed in (
                "Network connection destroyed: source=1, house=Some(1), reason=1, native_frame=1, startup_pending=false\n"
                "Could not load map: initial active roster is not a majority of the original admission\n"
                "Returned to the shell\n",
                "Network connection destroyed: source=1, house=Some(1), reason=1, native_frame=0, startup_pending=false\n"
                "Could not load map: unrelated startup failure\n"
                "Returned to the shell\n",
            ):
                with self.subTest(log=changed):
                    log_path.write_text(changed, encoding="utf-8")
                    checks = acceptance._a03_host_startup_log_checks(log_path)
                    self.assertTrue(any(not check.ok for check in checks))

    def test_a03_marker_and_fixture_restoration_match_source_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            series = root / "LDA-Continue-real"
            (series / "stage01").mkdir(parents=True)
            (series / "stage02").mkdir()
            (series / "campaign.ini").write_text(
                "[Campaign]\nScenarioCount=2\n"
                "Scenario1=stage01/all01umd.map\n"
                "Scenario2=stage02/all01umd.map\n",
                encoding="utf-8",
            )
            (series / "stage01" / "all01umd.map").write_bytes(b"selected map bytes")
            (series / "stage02" / "all01umd.map").write_bytes(b"later map bytes")
            isolated_settings = []
            for role in ("host", "guest1"):
                settings = root / role / "settings"
                copied = settings / "Maps" / "VMCE" / series.name
                copied.parent.mkdir(parents=True)
                shutil.copytree(series, copied)
                isolated_settings.append(SimpleNamespace(role=role, settings=settings))

            marker_path = root / "rendezvous" / "guest-mutation.json"
            marker_path.parent.mkdir()
            campaign_hash = hashlib.sha256((series / "campaign.ini").read_bytes()).hexdigest()
            selected_hash = hashlib.sha256(
                (series / "stage01" / "all01umd.map").read_bytes()
            ).hexdigest()
            later_hash = hashlib.sha256(
                (series / "stage02" / "all01umd.map").read_bytes()
            ).hexdigest()
            mutated_hash = hashlib.sha256(b"later map byteschanged").hexdigest()
            marker = {
                "schema": acceptance.A03_MARKER_SCHEMA,
                "ready_generation_before": 5,
                "ready_generation_after": 6,
                "selected_map": f"VMCE/{series.name}/stage01/all01umd.map",
                "unselected_later_map": "stage02/all01umd.map",
                "campaign_sha256_before": campaign_hash,
                "campaign_sha256_after": campaign_hash,
                "selected_sha256_before": selected_hash,
                "selected_sha256_after": selected_hash,
                "unselected_sha256_before": later_hash,
                "unselected_sha256_mutated": mutated_hash,
                "mutation_after_admitted_ready": True,
            }
            marker_bytes = json.dumps(marker, separators=(",", ":")).encode("utf-8")
            marker_path.write_bytes(marker_bytes)
            marker_sha = hashlib.sha256(marker_bytes).hexdigest()
            receipts = self.make_receipts(marker_path)
            host_a03 = receipts[0][1]["lan"]["a03"]
            guest_a03 = receipts[1][1]["lan"]["a03"]
            guest_a03["selected_map"] = marker["selected_map"]
            for a03 in (host_a03, guest_a03):
                a03["rendezvous_marker_sha256"] = marker_sha
            guest_a03.update(
                campaign_sha256_before=campaign_hash,
                campaign_sha256_after=campaign_hash,
                selected_sha256_before=selected_hash,
                selected_sha256_after=selected_hash,
                unselected_sha256_before=later_hash,
                unselected_sha256_mutated=mutated_hash,
                unselected_sha256_restored=later_hash,
            )
            checks = acceptance._a03_sandbox_restore_checks(
                receipts,
                isolated_settings,
                series_source=series,
                series_name=series.name,
                expected_entry="stage01/all01umd.map",
                marker_path=marker_path,
            )
            assert_all_pass(self, checks)

            marker_path.unlink()
            missing_marker_checks = acceptance._a03_sandbox_restore_checks(
                receipts,
                isolated_settings,
                series_source=series,
                series_name=series.name,
                expected_entry="stage01/all01umd.map",
                marker_path=marker_path,
            )
            self.assertTrue(any(not check.ok for check in missing_marker_checks))
            marker_path.write_bytes(marker_bytes)

            later_copy = (
                isolated_settings[1].settings
                / "Maps"
                / "VMCE"
                / series.name
                / "stage02"
                / "all01umd.map"
            )
            later_copy.write_bytes(b"left mutated")
            failed_checks = acceptance._a03_sandbox_restore_checks(
                receipts,
                isolated_settings,
                series_source=series,
                series_name=series.name,
                expected_entry="stage01/all01umd.map",
                marker_path=marker_path,
            )
            self.assertTrue(any(not check.ok for check in failed_checks))


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
