#!/usr/bin/env python3
"""Run and compare VMCE LAN live-capture acceptance bundles.

The tool drives the real ``lan-live-vera-campaign-host`` and
``lan-live-vera-campaign-guest`` shell-capture checkpoints.  It does not
simulate the game and does not read gameplay state itself: each process writes
an ordinary ``capture.json`` bundle and the comparator below checks the real
receipt fields.

Usage::

    python -m tools.vmce_lan_acceptance run --exe PATH \
        --retail-source DIR --series DIR [--suite fast|smoke|full]
    python -m tools.vmce_lan_acceptance compare --players 2 \
        --receipt HOST/capture.json --receipt GUEST/capture.json \
        [--expect-victory] [--expect-can-continue] [--expect-rows 2] \
        [--expect-next-stage] [--expect-screenshot NAME] \
        [--allow-missing-simulation-hashes]

``run`` is the default subcommand when the first argument is not ``compare``.
The v1 suites are:

* ``fast``: ``4p-win``.
* ``smoke``: ``4p-win`` and ``2p-win``.
* ``full``: the same scenarios as ``smoke`` in v1.  Defeat, final, houses and
  mismatch runs need different authored series/maps and are reserved names.

Every scenario reuses the ``--series``/``--entry`` map.  ``--players`` selects
the row count for the ``4p-*`` scenarios (2 or 4); ``2p-win`` always runs two
processes.  Scenarios run sequentially; the processes inside a scenario run
concurrently.

Process layout (all under ``--work-dir``)::

    <scenario>/<role>/
        retail/          retail copy used by this process (removed on success)
        settings/        cwd, config.toml, marker, Maps/VMCE/<series> (removed
                         on success)
        out/             immutable capture bundle, kept on success
        <role>.stdout.log
        <role>.stderr.log

Exit status is 0 when every check passes, 1 for a check/run failure, and 2 for
invalid arguments or setup (bad paths, UDP port conflict, copy failure).
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path, PurePosixPath
import shutil
import signal
import socket
import stat
import subprocess
import sys
import time
from typing import Any, Iterable, Mapping, Sequence


CAPTURE_ROOT_MARKER = ".vera20k-lan-capture-root"
CAPTURE_ROOT_BYTES = b"VERA20k isolated shell capture fixture\n"

DEFAULT_ENTRY = "stage01/all01umd.map"
DEFAULT_PORT_BASE = 32100
PORT_STRIDE = 10
HOST_READY_SECONDS = 15.0
MIN_NATIVE_CHECKSUMS = 300
POST_KILL_WAIT_SECONDS = 10.0

CAPTURE_WIDTH = 800
CAPTURE_HEIGHT = 600
CAPTURE_CURSOR_X = 400
CAPTURE_CURSOR_Y = 300

CHECKPOINT_HOST = "lan-live-vera-campaign-host"
CAPTURE_TIMEOUT_SECONDS = int(os.environ.get("VMCE_CAPTURE_TIMEOUT", "300"))

CHECKPOINT_GUEST = "lan-live-vera-campaign-guest"

# A host room is normally advertised within a few render frames.  The current
# capture path logs at debug level for its event stream, so these markers are
# best-effort; timing out this wait is not a failure and guests start anyway.
ROOM_CREATED_MARKERS = (
    "live LAN LiveVmceHost",
    "LAN transport",
    "LAN game",
    "room created",
    "Created game",
)


class AcceptanceError(Exception):
    """Base class for expected tool errors."""


class UsageError(AcceptanceError):
    """Invalid command line or an unsupported v1 combination."""


class SetupError(AcceptanceError):
    """The run could not be set up (paths, ports, copies)."""


class RunFailure(AcceptanceError):
    """A launched scenario failed before its receipts could be accepted."""


@dataclass(frozen=True)
class Check:
    """One printed acceptance check."""

    name: str
    ok: bool
    detail: str = ""


@dataclass(frozen=True)
class Scenario:
    """One built-in v1 scenario."""

    name: str
    players: int | None
    expect_victory: bool
    expect_can_continue: bool
    expect_next_stage: bool
    checkpoint_host: str = CHECKPOINT_HOST
    checkpoint_guest: str = CHECKPOINT_GUEST
    expect_retry: bool = False
    expect_screenshots: tuple[str, ...] = ()
    language: str = ""


SCENARIOS = {
    "4p-win": Scenario(
        name="4p-win",
        players=None,
        expect_victory=True,
        expect_can_continue=True,
        expect_next_stage=True,
    ),
    "2p-win": Scenario(
        name="2p-win",
        players=2,
        expect_victory=True,
        expect_can_continue=True,
        expect_next_stage=True,
    ),
    # Real two-process defeat/retry: both observe, BC6 opens on both, both
    # press Continue (guests vote, host starts once every online guest voted),
    # and the retried mission reloads the same stage with 300 equal checksums.
    "2p-retry": Scenario(
        name="2p-retry",
        players=2,
        expect_victory=False,
        expect_can_continue=True,
        expect_next_stage=False,
        expect_retry=True,
        expect_screenshots=("vera-result-real-lose", "vera-retry-mission"),
        language="zh-CN",
    ),
}

SUITES = {
    "dev": ("2p-retry",),
    "fast": ("4p-win",),
    "smoke": ("4p-win", "2p-win", "2p-retry"),
    "full": ("4p-win", "2p-win", "2p-retry"),
}

VMCE_CHECKPOINT_PREFIX = "lan-live-vera-"


def validate_scenario_checkpoints(scenario: Scenario) -> None:
    """Reject a scenario that points at a non-VMCE live checkpoint.

    All acceptance scenarios are VMCE receipts: a classic Cooperative
    checkpoint would run the stock two-row page and cannot prove the VMCE
    BC6 four-row vote/retry path. Fail before any process is launched.
    """
    for role, checkpoint in (
        ("host", scenario.checkpoint_host),
        ("guest", scenario.checkpoint_guest),
    ):
        if not checkpoint.startswith(VMCE_CHECKPOINT_PREFIX):
            raise UsageError(
                f"scenario {scenario.name!r} uses the non-VMCE {role} checkpoint "
                f"{checkpoint!r}; VMCE acceptance requires "
                f"{VMCE_CHECKPOINT_PREFIX}* checkpoints"
            )


RESERVED_SCENARIOS = {
    "4p-lose": "defeat acceptance needs a loss-entry series and is not implemented in v1",
    "2p-lose": "defeat acceptance needs a loss-entry series and is not implemented in v1",
    "4p-final": "campaign-final acceptance needs a final-stage series and is not implemented in v1",
    "2p-final": "campaign-final acceptance needs a final-stage series and is not implemented in v1",
    "4p-houses": "houses acceptance needs the houses-entry series and is not implemented in v1",
    "2p-houses": "houses acceptance needs the houses-entry series and is not implemented in v1",
    "mismatch": "asymmetric mismatch acceptance is not implemented in v1",
}


@dataclass
class ProcessSpec:
    """Paths and command line for one capture process."""

    role: str
    index: int
    bind: str
    peers: tuple[str, ...]
    directory: Path
    settings: Path
    retail: Path
    output: Path
    stdout_path: Path
    stderr_path: Path
    command: list[str]


@dataclass
class LaunchedProcess:
    """A started process and its redirected log handles."""

    spec: ProcessSpec
    popen: subprocess.Popen[bytes]
    stdout_file: Any
    stderr_file: Any
    timed_out: bool = False

    def close(self) -> None:
        for handle in (self.stdout_file, self.stderr_file):
            try:
                handle.close()
            except OSError:
                pass


@dataclass
class ScenarioResult:
    """Collected checks and process records for one scenario."""

    name: str
    players: int
    checks: list[Check]
    processes: list[dict[str, Any]]
    error: str | None = None
    cleaned: bool = False
    host_room_marker: bool = False
    specs: list[ProcessSpec] = field(default_factory=list, repr=False)

    @property
    def passed(self) -> bool:
        return self.error is None and all(check.ok for check in self.checks)

    def as_json(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "players": self.players,
            "status": "pass" if self.passed else "fail",
            "error": self.error,
            "host_room_marker": self.host_room_marker,
            "cleaned": self.cleaned,
            "checks": [
                {"name": check.name, "ok": check.ok, "detail": check.detail}
                for check in self.checks
            ],
            "processes": self.processes,
        }


@dataclass(frozen=True)
class RunContext:
    """Validated inputs shared by every scenario in one run."""

    exe: Path
    retail_source: Path
    series: Path
    series_name: str
    entry: str
    map_name: str
    work_dir: Path
    port_base: int
    timeout_seconds: float
    keep: bool
    players: int
    suite: str


@dataclass(frozen=True)
class KillPlan:
    """How a timed-out process tree should be terminated on this host."""

    kind: str
    argv: tuple[str, ...] = ()


# ---------------------------------------------------------------------------
# Small pure helpers
# ---------------------------------------------------------------------------


def validate_players(players: Any) -> int:
    """Return a supported player count, or raise ``UsageError``."""
    if isinstance(players, bool) or not isinstance(players, int):
        raise UsageError(f"--players must be 2 or 4, got {players!r}")
    if players not in (2, 4):
        raise UsageError(f"--players must be 2 or 4, got {players}")
    return players


def validate_timeout_seconds(value: Any) -> float:
    """Return a finite positive timeout, or raise ``UsageError``."""
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise UsageError(f"--timeout-seconds must be a positive number, got {value!r}")
    if not math.isfinite(value) or value <= 0:
        raise UsageError(f"--timeout-seconds must be a finite positive number, got {value!r}")
    return float(value)


def validate_peer_count(players: int, role: str, peers: Sequence[Any]) -> None:
    """Check the endpoint count the capture CLI requires for one role."""
    players = validate_players(players)
    if role not in ("host", "guest"):
        raise UsageError(f"unknown process role {role!r}")
    expected = players - 1 if role == "host" else 1
    if len(peers) != expected:
        raise UsageError(
            f"{role} needs {expected} --lan-peer endpoint(s) for {players} players, "
            f"got {len(peers)}"
        )


def validate_port_base(port_base: int, players: int) -> int:
    """Return a valid base port with room for every bind and its +2 alternate."""
    players = validate_players(players)
    if isinstance(port_base, bool) or not isinstance(port_base, int):
        raise UsageError(f"--port-base must be an integer, got {port_base!r}")
    if port_base < 1:
        raise UsageError(f"--port-base must be greater than zero, got {port_base}")
    last_bind = port_base + (players - 1) * PORT_STRIDE
    if last_bind + 2 > 65535:
        raise UsageError(
            f"--port-base {port_base} leaves no room for {players} processes "
            f"and their +2 alternate ports (last bind {last_bind})"
        )
    return port_base


def process_ports(port_base: int, players: int) -> tuple[int, ...]:
    """Return each process's primary bind port."""
    validate_port_base(port_base, players)
    return tuple(port_base + index * PORT_STRIDE for index in range(players))


def build_lan_map_name(series: Path, entry: str) -> str:
    """Return the ``VMCE/<series>/<entry>`` catalog name."""
    parts = PurePosixPath(entry.replace("\\", "/")).parts
    if not parts:
        raise UsageError(f"--entry must not be empty: {entry!r}")
    return "/".join(("VMCE", series.name, *parts))


def validate_entry(series: Path, entry: str) -> str:
    """Validate a relative map entry inside the series and return it normalized."""
    normalized = entry.replace("\\", "/")
    path = PurePosixPath(normalized)
    if path.is_absolute() or not path.parts or any(part in ("", ".", "..") for part in path.parts):
        raise UsageError(f"--entry must be a relative path inside --series, got {entry!r}")
    candidate = series.joinpath(*path.parts)
    if not candidate.is_file():
        raise UsageError(f"--entry does not name a file inside --series: {candidate}")
    return normalized


def validate_jobs(jobs: int) -> int:
    """v1 has no scenario parallelism; reserve ``--jobs`` with a clear error."""
    if isinstance(jobs, bool) or not isinstance(jobs, int) or jobs < 1:
        raise UsageError(f"--jobs must be a positive integer, got {jobs!r}")
    if jobs != 1:
        raise UsageError(
            "--jobs > 1 is not supported: scenario parallelism not enabled in v1"
        )
    return jobs


def selected_scenarios(args: argparse.Namespace) -> list[Scenario]:
    """Resolve ``--scenario``/``--suite`` (including reserved v1 names)."""
    if args.scenario:
        names = list(args.scenario)
    else:
        names = list(SUITES[args.suite])
    seen: set[str] = set()
    chosen: list[Scenario] = []
    for name in names:
        if name in RESERVED_SCENARIOS:
            raise UsageError(f"scenario {name!r}: {RESERVED_SCENARIOS[name]}")
        scenario = SCENARIOS.get(name)
        if scenario is None:
            known = ", ".join(sorted(SCENARIOS))
            raise UsageError(f"unknown scenario {name!r}; v1 supports: {known}")
        if name in seen:
            raise UsageError(f"scenario {name!r} was selected more than once")
        seen.add(name)
        validate_scenario_checkpoints(scenario)
        chosen.append(scenario)
    if not chosen:
        raise UsageError("no scenarios selected")
    return chosen


def kill_tree_plan(pid: int, platform: str | None = None) -> KillPlan:
    """Return the platform process-tree kill command for ``pid``."""
    platform = sys.platform if platform is None else platform
    if platform.startswith("win"):
        return KillPlan("taskkill", ("taskkill", "/PID", str(pid), "/T", "/F"))
    return KillPlan("killpg")


def is_timeout(elapsed_seconds: float, timeout_seconds: float) -> bool:
    """Whether a wait has reached its deadline."""
    return elapsed_seconds >= timeout_seconds


def host_room_marker_seen(text: str) -> bool:
    """Whether capture output contains one of the best-effort room markers."""
    folded = text.casefold()
    return any(marker.casefold() in folded for marker in ROOM_CREATED_MARKERS)


def format_value(value: Any, limit: int = 96) -> str:
    """Compact JSON-ish rendering for a check detail."""
    try:
        text = json.dumps(value, ensure_ascii=False, sort_keys=True)
    except (TypeError, ValueError):
        text = repr(value)
    if len(text) > limit:
        text = text[: limit - 3] + "..."
    return text


def _toml_escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"')


# ---------------------------------------------------------------------------
# Receipt field access and comparison
# ---------------------------------------------------------------------------


def receipt_field(receipt: Mapping[str, Any], path: str) -> Any:
    """Read a dotted path from a receipt, raising ``KeyError`` when missing."""
    value: Any = receipt
    for part in path.split("."):
        if not isinstance(value, Mapping) or part not in value:
            raise KeyError(path)
        value = value[part]
    return value


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _is_str(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _normalise(value: Any) -> Any:
    """Turn lists/dicts into hashable comparisons without changing values."""
    if isinstance(value, Mapping):
        return tuple(sorted((str(key), _normalise(item)) for key, item in value.items()))
    if isinstance(value, (list, tuple)):
        return tuple(_normalise(item) for item in value)
    return value


def _first_difference(
    baseline: Sequence[Any], other: Sequence[Any]
) -> tuple[int, Any, Any] | None:
    for index, (left, right) in enumerate(zip(baseline, other)):
        if left != right:
            return index, left, right
    if len(baseline) != len(other):
        index = min(len(baseline), len(other))
        left = baseline[index] if index < len(baseline) else None
        right = other[index] if index < len(other) else None
        return index, left, right
    return None


def _equal_check(
    receipts: Sequence[tuple[str, Mapping[str, Any]]],
    path: str,
    name: str,
    *,
    expected: Any = None,
    value_type: type | None = None,
) -> Check:
    """Require one field in every receipt, equal across them, and optionally expected."""
    values: list[Any] = []
    for label, receipt in receipts:
        try:
            value = receipt_field(receipt, path)
        except KeyError:
            return Check(name, False, f"missing {path} in {label}")
        if value_type is int and not _is_int(value):
            return Check(name, False, f"{path} is not an integer in {label}: {format_value(value)}")
        if value_type is bool and not isinstance(value, bool):
            return Check(name, False, f"{path} is not a boolean in {label}: {format_value(value)}")
        if value_type is str and not _is_str(value):
            return Check(name, False, f"{path} is not a non-empty string in {label}: {format_value(value)}")
        values.append(value)

    baseline = values[0]
    for (label, _), value in zip(receipts[1:], values[1:]):
        if _normalise(value) != _normalise(baseline):
            return Check(
                name,
                False,
                f"{path}: {receipts[0][0]}={format_value(baseline)} "
                f"{label}={format_value(value)}",
            )
    if expected is not None and _normalise(baseline) != _normalise(expected):
        return Check(
            name,
            False,
            f"{path}: expected {format_value(expected)}, got {format_value(baseline)}",
        )
    return Check(name, True, f"{path} = {format_value(baseline)}")


def _checksum_count_check(receipts: Sequence[tuple[str, Mapping[str, Any]]]) -> Check:
    counts: list[int] = []
    for label, receipt in receipts:
        try:
            value = receipt_field(receipt, "lan.native_checksums")
        except KeyError:
            return Check(
                "native_checksums count", False, f"missing lan.native_checksums in {label}"
            )
        if not isinstance(value, list):
            return Check(
                "native_checksums count",
                False,
                f"lan.native_checksums is not a list in {label}: {format_value(value)}",
            )
        if len(value) < MIN_NATIVE_CHECKSUMS:
            return Check(
                "native_checksums count",
                False,
                f"lan.native_checksums has {len(value)} entries in {label}; "
                f"need at least {MIN_NATIVE_CHECKSUMS}",
            )
        counts.append(len(value))
    return Check(
        "native_checksums count",
        True,
        f"{format_value(counts)} entries (minimum {MIN_NATIVE_CHECKSUMS})",
    )


def _checksum_equal_check(receipts: Sequence[tuple[str, Mapping[str, Any]]]) -> Check:
    lists: list[tuple[str, Any]] = []
    for label, receipt in receipts:
        try:
            value = receipt_field(receipt, "lan.native_checksums")
        except KeyError:
            return Check("native_checksums equal", False, f"missing lan.native_checksums in {label}")
        if not isinstance(value, list):
            return Check(
                "native_checksums equal",
                False,
                f"lan.native_checksums is not a list in {label}",
            )
        lists.append((label, _normalise(value)))
    baseline_label, baseline = lists[0]
    baseline_seq = tuple(baseline)
    for label, value in lists[1:]:
        difference = _first_difference(baseline_seq, tuple(value))
        if difference is not None:
            index, left, right = difference
            return Check(
                "native_checksums equal",
                False,
                f"lan.native_checksums[{index}]: {baseline_label}={format_value(left)} "
                f"{label}={format_value(right)}",
            )
    return Check(
        "native_checksums equal",
        True,
        f"all {len(lists)} receipts share {len(baseline_seq)} entries",
    )


def _simulation_hash_checks(
    receipts: Sequence[tuple[str, Mapping[str, Any]]],
    allow_missing: bool,
) -> list[Check]:
    values: list[tuple[str, Any]] = []
    missing: list[str] = []
    for label, receipt in receipts:
        try:
            value = receipt_field(receipt, "lan.simulation_hashes")
        except KeyError:
            missing.append(label)
            continue
        if not isinstance(value, list) or not value:
            missing.append(label)
            continue
        values.append((label, _normalise(value)))

    if missing:
        if not allow_missing:
            return [
                Check(
                    "simulation_hashes",
                    False,
                    "missing or empty lan.simulation_hashes in "
                    + ", ".join(missing)
                    + " (pass --allow-missing-simulation-hashes to accept)",
                ),
                Check(
                    "simulation_hashes equal",
                    False,
                    "cannot compare while a receipt has no simulation hashes",
                ),
            ]
        return [
            Check(
                "simulation_hashes",
                True,
                "missing or empty in " + ", ".join(missing) + " (allowed)",
            ),
            Check("simulation_hashes equal", True, "skipped (missing hashes allowed)"),
        ]

    baseline_label, baseline = values[0]
    baseline_seq = tuple(baseline)
    for label, value in values[1:]:
        difference = _first_difference(baseline_seq, tuple(value))
        if difference is not None:
            index, left, right = difference
            return [
                Check("simulation_hashes", True, f"present in all {len(values)} receipts"),
                Check(
                    "simulation_hashes equal",
                    False,
                    f"lan.simulation_hashes[{index}]: {baseline_label}="
                    f"{format_value(left)} {label}={format_value(right)}",
                ),
            ]
    return [
        Check("simulation_hashes", True, f"present in all {len(values)} receipts"),
        Check("simulation_hashes equal", True, f"{len(baseline_seq)} entries"),
    ]


def _next_mission_check(
    receipts: Sequence[tuple[str, Mapping[str, Any]]], expect_next_stage: bool
) -> Check:
    path = "lan.vera_outcome.next_mission.map"
    maps: list[tuple[str, Any]] = []
    missing: list[str] = []
    for label, receipt in receipts:
        try:
            next_mission = receipt_field(receipt, "lan.vera_outcome.next_mission")
            if not isinstance(next_mission, Mapping) or "map" not in next_mission:
                missing.append(label)
                continue
            value = next_mission["map"]
        except KeyError:
            missing.append(label)
            continue
        if not _is_str(value):
            return Check("next_mission.map", False, f"{path} is empty or not text in {label}")
        maps.append((label, value))

    if missing:
        if len(missing) == len(receipts) and not expect_next_stage:
            return Check("next_mission.map", True, f"{path} not present")
        return Check(
            "next_mission.map",
            False,
            f"missing {path} in " + ", ".join(missing),
        )

    baseline_label, baseline = maps[0]
    for label, value in maps[1:]:
        if value != baseline:
            return Check(
                "next_mission.map",
                False,
                f"{path}: {baseline_label}={format_value(baseline)} "
                f"{label}={format_value(value)}",
            )
    return Check(
        "next_mission.map",
        True,
        f"{path} = {format_value(baseline)}",
    )


def _screenshot_names(
    receipts: Sequence[tuple[str, Mapping[str, Any]]],
) -> tuple[Check, dict[str, list[str]]]:
    names: dict[str, list[str]] = {}
    for label, receipt in receipts:
        try:
            screenshots = receipt_field(receipt, "screenshots")
        except KeyError:
            return Check("screenshots", False, f"missing screenshots in {label}"), {}
        if not isinstance(screenshots, list) or not screenshots:
            return Check("screenshots", False, f"screenshots is empty in {label}"), {}
        labels: list[str] = []
        for index, entry in enumerate(screenshots):
            if not isinstance(entry, Mapping):
                return Check(
                    "screenshots",
                    False,
                    f"screenshots[{index}] is not an object in {label}",
                ), {}
            name = entry.get("name")
            if not _is_str(name):
                return Check(
                    "screenshots",
                    False,
                    f"screenshots[{index}].name is empty or not text in {label}",
                ), {}
            labels.append(name)
        names[label] = labels
    return Check(
        "screenshots",
        True,
        "; ".join(f"{label}: {', '.join(values)}" for label, values in names.items()),
    ), names


def _series_sha_check(receipts: Sequence[tuple[str, Mapping[str, Any]]]) -> Check:
    path = "lan.vmce.series_sha256"
    values: list[tuple[str, str]] = []
    missing: list[str] = []
    for label, receipt in receipts:
        try:
            value = receipt_field(receipt, path)
        except KeyError:
            missing.append(label)
            continue
        if not _is_str(value):
            return Check("series_sha256", False, f"{path} is empty or not text in {label}")
        values.append((label, value))
    if not values:
        return Check("series_sha256", True, f"{path} not present")
    if missing:
        return Check(
            "series_sha256",
            False,
            f"{path} is present in some receipts but missing in " + ", ".join(missing),
        )
    baseline_label, baseline = values[0]
    for label, value in values[1:]:
        if value != baseline:
            return Check(
                "series_sha256",
                False,
                f"{path}: {baseline_label}={format_value(baseline)} "
                f"{label}={format_value(value)}",
            )
    return Check("series_sha256", True, f"{path} = {format_value(baseline)}")


def _retry_checks(receipts: Sequence[tuple[str, Mapping[str, Any]]]) -> list[Check]:
    """Both retry receipts loaded the same stage and ran 300 equal checksums."""
    checks: list[Check] = []
    baseline_label, baseline = receipts[0]
    # The classic co-op path carries its retried-mission ring; the VMCE
    # outcome path records the retry evidence under vera_outcome and keeps the
    # 300-frame native checksum prefix in the shared receipt.
    vmce = False
    try:
        receipt_field(baseline, "lan.vera_outcome.kind")
        vmce = True
    except KeyError:
        pass
    if vmce:
        try:
            baseline_kind = receipt_field(baseline, "lan.vera_outcome.kind")
            baseline_checksums = receipt_field(baseline, "lan.native_checksums")
            baseline_map = receipt_field(
                baseline, "lan.vera_outcome.retry_mission.map"
            )
        except KeyError as exc:
            return [Check("retry receipt", False, f"{baseline_label} missing {exc}")]
        if baseline_kind != "retry":
            return [
                Check(
                    "retry receipt",
                    False,
                    f"{baseline_label} vera_outcome.kind={format_value(baseline_kind)}",
                )
            ]
    else:
        try:
            baseline_checksums = receipt_field(baseline, "lan.cooperative.retry_checksums")
            baseline_map = receipt_field(baseline, "lan.cooperative.retry_startup.map")
        except KeyError as exc:
            return [Check("retry receipt", False, f"{baseline_label} missing {exc}")]
    if not isinstance(baseline_checksums, list) or len(baseline_checksums) < MIN_NATIVE_CHECKSUMS:
        return [
            Check(
                "retry checksums count",
                False,
                f"{baseline_label} retry_checksums={format_value(baseline_checksums)}",
            )
        ]
    checks.append(
        Check(
            "retry checksums count",
            True,
            f"{len(baseline_checksums)} entries",
        )
    )
    for label, receipt in receipts:
        if vmce:
            try:
                victory = receipt_field(receipt, "lan.vera_outcome.dialog.victory")
                retry_map = receipt_field(
                    receipt, "lan.vera_outcome.retry_mission.map"
                )
                checksums = receipt_field(receipt, "lan.native_checksums")
                failure = receipt_field(receipt, "lan.checksum_failure")
            except KeyError as exc:
                checks.append(Check("retry receipt", False, f"{label} missing {exc}"))
                continue
            checks.append(
                Check(
                    f"retry verdict {label}",
                    victory is False,
                    f"victory={format_value(victory)}",
                )
            )
            checks.append(
                Check(
                    f"retry stage {label}",
                    _is_str(retry_map),
                    f"retry_map={format_value(retry_map)}",
                )
            )
            checks.append(
                Check(
                    f"retry checksum failure {label}",
                    failure == "None",
                    f"checksum_failure={format_value(failure)}",
                )
            )
        else:
            try:
                victory = receipt_field(receipt, "lan.cooperative.result.victory")
                retry_map = receipt_field(receipt, "lan.cooperative.retry_startup.map")
                first_map = receipt_field(receipt, "lan.cooperative.first_map")
                checksums = receipt_field(receipt, "lan.cooperative.retry_checksums")
                failure = receipt_field(receipt, "lan.cooperative.checksum_failure")
            except KeyError as exc:
                checks.append(Check("retry receipt", False, f"{label} missing {exc}"))
                continue
            checks.append(
                Check(
                    f"retry verdict {label}",
                    victory is False,
                    f"victory={format_value(victory)}",
                )
            )
            checks.append(
                Check(
                    f"retry stage {label}",
                    retry_map == first_map,
                    f"first={format_value(first_map)} retry={format_value(retry_map)}",
                )
            )
            checks.append(
                Check(
                    f"retry checksum failure {label}",
                    failure == "None",
                    f"checksum_failure={format_value(failure)}",
                )
            )
        if label != baseline_label:
            checks.append(
                Check(
                    "retry checksums equal",
                    checksums == baseline_checksums,
                    f"{baseline_label}={len(baseline_checksums)} entries, "
                    f"{label}={len(checksums) if isinstance(checksums, list) else '?'} entries",
                )
            )
            if vmce:
                checks.append(
                    Check(
                        "retry stage equal",
                        retry_map == baseline_map,
                        f"{baseline_label}={format_value(baseline_map)} "
                        f"{label}={format_value(retry_map)}",
                    )
                )
    return checks


def _retry_identity_check(
    label: str,
    receipt: Mapping[str, Any],
    expected_checkpoint: str,
    _expected_bind: str,
    expected_map: str,
) -> Check:
    # The co-op receipt is produced by the shared capture receipt builder, not
    # the VMCE lobby receipt; it proves the checkpoint and the clean checksum
    # stream instead of carrying lan.bind.
    expectations = [("checkpoint", expected_checkpoint)]
    for path, expected in expectations:
        try:
            value = receipt_field(receipt, path)
        except KeyError:
            return Check(f"identity {label}", False, f"missing {path}")
        if _normalise(value) != _normalise(expected):
            return Check(
                f"identity {label}",
                False,
                f"{path}: expected {format_value(expected)}, got {format_value(value)}",
            )
    try:
        retry_map = receipt_field(receipt, "lan.cooperative.retry_startup.map")
    except KeyError:
        try:
            retry_map = receipt_field(receipt, "lan.vera_outcome.retry_mission.map")
        except KeyError:
            return Check(f"identity {label}", False, "missing retry mission map")
    if not _is_str(retry_map):
        return Check(f"identity {label}", False, f"retry map={format_value(retry_map)}")
    return Check(f"identity {label}", True, f"checkpoint={expected_checkpoint}")


def evaluate_receipts(
    receipts: Sequence[tuple[str, Mapping[str, Any]]],
    *,
    players: int,
    expect_victory: bool | None = None,
    expect_can_continue: bool | None = None,
    expect_rows: int | None = None,
    expect_next_stage: bool = False,
    expect_retry: bool = False,
    expect_screenshots: Sequence[str] = (),
    allow_missing_simulation_hashes: bool = False,
) -> list[Check]:
    """Compare loaded receipts using the real VMCE capture fields.

    ``receipts`` is ``(label, parsed-capture.json)`` in host-first order.  The
    returned checks are pure results; printing and exit codes belong to main.
    """
    players = validate_players(players)
    if len(receipts) != players:
        return [
            Check(
                "receipts",
                False,
                f"expected {players} receipts, got {len(receipts)}",
            )
        ]
    checks: list[Check] = [Check("receipts", True, f"{players} receipts")]

    if expect_retry:
        checks.extend(_retry_checks(receipts))
    else:
        checks.append(_checksum_count_check(receipts))
        checks.append(_checksum_equal_check(receipts))
        checks.extend(_simulation_hash_checks(receipts, allow_missing_simulation_hashes))

    if expect_retry:
        checks.append(_screenshot_names(receipts)[0])
        for expected_name in expect_screenshots:
            for label, available in _screenshot_names(receipts)[1].items():
                if expected_name not in available:
                    checks.append(
                        Check(
                            f"screenshot {expected_name}",
                            False,
                            f"{label} has no screenshot named {expected_name!r}",
                        )
                    )
                    break
            else:
                checks.append(
                    Check(f"screenshot {expected_name}", True, "present in every receipt")
                )
        checks.append(_series_sha_check(receipts))
        return checks

    checks.append(
        _equal_check(
            receipts,
            "lan.vera_outcome.dialog.victory",
            "dialog.victory",
            expected=expect_victory,
            value_type=bool,
        )
    )
    checks.append(
        _equal_check(
            receipts,
            "lan.vera_outcome.dialog.rows",
            "dialog.rows",
            expected=expect_rows,
            value_type=int,
        )
    )
    checks.append(
        _equal_check(
            receipts,
            "lan.vera_outcome.dialog.can_continue",
            "dialog.can_continue",
            expected=expect_can_continue,
            value_type=bool,
        )
    )

    checks.append(_next_mission_check(receipts, expect_next_stage))
    checks.append(
        _equal_check(receipts, "lan.startup.seed", "seed", value_type=int)
    )
    checks.append(
        _equal_check(receipts, "lan.vera_outcome.state_hash", "state_hash", value_type=int)
    )

    screenshot_check, names = _screenshot_names(receipts)
    checks.append(screenshot_check)
    for expected_name in expect_screenshots:
        for label, available in names.items():
            if expected_name not in available:
                checks.append(
                    Check(
                        f"screenshot {expected_name}",
                        False,
                        f"{label} has no screenshot named {expected_name!r}",
                    )
                )
                break
        else:
            checks.append(
                Check(f"screenshot {expected_name}", True, "present in every receipt")
            )

    checks.append(_series_sha_check(receipts))
    return checks


# ---------------------------------------------------------------------------
# Settings fixture
# ---------------------------------------------------------------------------


def write_process_settings(
    settings_dir: Path,
    retail_copy: Path,
    series_source: Path,
) -> Path:
    """Create one isolated process settings tree and return its config path.

    The caller owns the retail copy.  This writes the exact isolated-root marker,
    a config pointing at that copy with forward slashes, and a per-process copy
    of the VMCE series at ``Maps/VMCE/<series-name>``.
    """
    settings_dir = Path(settings_dir)
    if settings_dir.exists():
        raise SetupError(f"settings directory already exists: {settings_dir}")
    settings_dir.mkdir(parents=True)

    ra2_dir = Path(retail_copy).resolve().as_posix()
    config = (
        "[paths]\n"
        f'ra2_dir = "{_toml_escape(ra2_dir)}"\n'
        'settings_dir = "."\n'
    )
    config_path = settings_dir / "config.toml"
    with config_path.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write(config)

    (settings_dir / CAPTURE_ROOT_MARKER).write_bytes(CAPTURE_ROOT_BYTES)

    series_name = Path(series_source).name
    if not series_name:
        raise SetupError(f"cannot derive a VMCE series name from {series_source}")
    maps_dir = settings_dir / "Maps" / "VMCE"
    maps_dir.mkdir(parents=True, exist_ok=True)
    destination = maps_dir / series_name
    if destination.exists():
        raise SetupError(f"VMCE series destination already exists: {destination}")
    try:
        shutil.copytree(series_source, destination)
    except OSError as exc:
        raise SetupError(f"failed to copy VMCE series {series_source}: {exc}") from exc
    return config_path


# ---------------------------------------------------------------------------
# Process orchestration
# ---------------------------------------------------------------------------


def _process_environment() -> dict[str, str]:
    """Environment for one capture process: config-only asset routing."""
    environment = os.environ.copy()
    for key in [key for key in environment if key.upper().startswith("RA2_")]:
        del environment[key]
    environment["RUST_LOG"] = os.environ.get("VMCE_RUST_LOG", "info")
    return environment


def build_capture_command(
    *,
    exe: Path,
    role: str,
    players: int,
    bind: str,
    peers: Sequence[str],
    output: Path,
    map_name: str | None,
    lobby_name: str | None,
    checkpoint_host: str = CHECKPOINT_HOST,
    checkpoint_guest: str = CHECKPOINT_GUEST,
    vera_outcome: str = "real",
    map_required: bool = True,
    language: str = "",
) -> list[str]:
    """Build the real shell-capture command line for one VMCE process."""
    if role not in ("host", "guest"):
        raise UsageError(f"unknown process role {role!r}")
    validate_peer_count(players, role, peers)
    command = [
        str(exe),
        "--shell-capture",
        checkpoint_host if role == "host" else checkpoint_guest,
        "--width",
        str(CAPTURE_WIDTH),
        "--height",
        str(CAPTURE_HEIGHT),
        "--cursor-x",
        str(CAPTURE_CURSOR_X),
        "--cursor-y",
        str(CAPTURE_CURSOR_Y),
        "--output",
        str(Path(output).resolve()),
        "--lan-bind",
        bind,
        "--lan-peer",
        ",".join(peers),
        "--lan-players",
        str(players),
    ]
    if role == "host" and map_required:
        if not map_name:
            raise UsageError("host capture requires a --lan-map catalog name")
        command.extend(("--lan-map", map_name))
    if lobby_name:
        command.extend(("--lan-name", lobby_name))
    # Long full-prefix runs stay the default; dev probes can shorten this
    # with VMCE_CAPTURE_TIMEOUT (seconds) instead of waiting out the
    # capture binary's own 600 s watchdog.
    command.extend(("--shell-capture-timeout-seconds", str(CAPTURE_TIMEOUT_SECONDS)))
    if vera_outcome:
        command.extend(("--vera-outcome", vera_outcome))
    if language:
        command.extend(("--language", language))
    return command


def _prepare_process(
    context: RunContext,
    scenario: Scenario,
    index: int,
    bind: str,
    peers: Sequence[str],
    players: int,
) -> ProcessSpec:
    role = "host" if index == 0 else f"guest{index}"
    directory = context.work_dir / scenario.name / role
    if directory.exists():
        raise SetupError(f"process directory already exists: {directory}")
    directory.mkdir(parents=True)
    retail_copy = directory / "retail"
    try:
        shutil.copytree(context.retail_source, retail_copy)
    except OSError as exc:
        raise SetupError(f"failed to copy retail source {context.retail_source}: {exc}") from exc
    settings = directory / "settings"
    write_process_settings(settings, retail_copy, context.series)
    output = directory / "out"
    lobby_name = None
    if players > 2:
        lobby_name = "LiveHost" if role == "host" else f"LiveGuest{index}"
    command = build_capture_command(
        exe=context.exe,
        role="host" if index == 0 else "guest",
        players=players,
        bind=bind,
        peers=peers,
        output=output,
        map_name=context.map_name if index == 0 else None,
        lobby_name=lobby_name,
        checkpoint_host=scenario.checkpoint_host,
        checkpoint_guest=scenario.checkpoint_guest,
        vera_outcome=(
            ("retry" if scenario.expect_retry else "real")
            if scenario.checkpoint_host in (CHECKPOINT_HOST, CHECKPOINT_GUEST)
            else ""
        ),
        map_required=scenario.checkpoint_host == CHECKPOINT_HOST,
        language=scenario.language,
    )
    return ProcessSpec(
        role=role,
        index=index,
        bind=bind,
        peers=tuple(peers),
        directory=directory,
        settings=settings,
        retail=retail_copy,
        output=output,
        stdout_path=directory / f"{role}.stdout.log",
        stderr_path=directory / f"{role}.stderr.log",
        command=command,
    )


def _launch_process(spec: ProcessSpec, environment: Mapping[str, str]) -> LaunchedProcess:
    stdout_file = spec.stdout_path.open("wb")
    stderr_file = spec.stderr_path.open("wb")
    kwargs: dict[str, Any] = {}
    if os.name != "nt":
        kwargs["start_new_session"] = True
    try:
        popen = subprocess.Popen(
            spec.command,
            cwd=spec.settings,
            env=dict(environment),
            stdin=subprocess.DEVNULL,
            stdout=stdout_file,
            stderr=stderr_file,
            shell=False,
            **kwargs,
        )
    except OSError as exc:
        stdout_file.close()
        stderr_file.close()
        raise RunFailure(f"failed to launch {spec.role}: {exc}") from exc
    return LaunchedProcess(spec=spec, popen=popen, stdout_file=stdout_file, stderr_file=stderr_file)


def _read_log_text(paths: Iterable[Path]) -> str:
    parts: list[str] = []
    for path in paths:
        try:
            parts.append(path.read_text(encoding="utf-8", errors="replace"))
        except OSError:
            pass
    return "".join(parts)


def _host_log_paths(process: LaunchedProcess) -> tuple[Path, ...]:
    return (
        process.spec.stdout_path,
        process.spec.stderr_path,
        process.spec.settings / "logs" / "ra2.log",
    )


def wait_for_host_ready(process: LaunchedProcess, timeout_seconds: float = HOST_READY_SECONDS) -> bool:
    """Wait for a best-effort room-created marker; return ``False`` on timeout."""
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        if host_room_marker_seen(_read_log_text(_host_log_paths(process))):
            return True
        if process.popen.poll() is not None:
            return host_room_marker_seen(_read_log_text(_host_log_paths(process)))
        time.sleep(0.1)
    return False


def kill_process_tree(process: LaunchedProcess) -> None:
    """Kill one process and its descendants, then wait for it to settle."""
    if process.popen.poll() is not None:
        return
    plan = kill_tree_plan(process.popen.pid)
    if plan.kind == "taskkill":
        try:
            subprocess.run(
                list(plan.argv),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
        except OSError:
            pass
    else:
        try:
            os.killpg(os.getpgid(process.popen.pid), signal.SIGKILL)
        except (OSError, ProcessLookupError):
            try:
                process.popen.kill()
            except OSError:
                pass
    try:
        process.popen.wait(timeout=POST_KILL_WAIT_SECONDS)
    except subprocess.TimeoutExpired:
        try:
            process.popen.kill()
        except OSError:
            pass


def wait_for_scenario(
    processes: Sequence[LaunchedProcess], timeout_seconds: float
) -> str | None:
    """Wait for all processes; return an error reason or ``None`` on success."""
    started = time.monotonic()
    while True:
        codes: dict[str, int | None] = {}
        for process in processes:
            code = process.popen.poll()
            codes[process.spec.role] = code
            if code is not None and code != 0:
                return f"{process.spec.role} exited with code {code} before the scenario finished"
        if all(code is not None for code in codes.values()):
            return None
        if is_timeout(time.monotonic() - started, timeout_seconds):
            for process in processes:
                if process.popen.poll() is None:
                    process.timed_out = True
            return f"scenario timed out after {timeout_seconds:g}s"
        time.sleep(0.2)


def _process_record(process: LaunchedProcess) -> dict[str, Any]:
    return {
        "role": process.spec.role,
        "pid": process.popen.pid,
        "bind": process.spec.bind,
        "peers": list(process.spec.peers),
        "output": str(process.spec.output),
        "command": list(process.spec.command),
        "exit_code": process.popen.returncode,
        "timed_out": process.timed_out,
        "stdout": str(process.spec.stdout_path),
        "stderr": str(process.spec.stderr_path),
    }


def _load_receipt(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise RunFailure(f"cannot read capture receipt {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise RunFailure(f"invalid JSON in capture receipt {path}: {exc}") from exc
    if not isinstance(data, dict):
        raise RunFailure(f"capture receipt is not a JSON object: {path}")
    return data


def _receipt_identity_check(
    label: str,
    receipt: Mapping[str, Any],
    expected_checkpoint: str,
    expected_bind: str,
    expected_map: str,
    is_host: bool,
) -> Check:
    """Check the capture identity fields that prove this process's role."""
    expectations: list[tuple[str, Any]] = [
        ("checkpoint", expected_checkpoint),
        ("lan.bind", expected_bind),
        ("lan.checksum_failure", "None"),
        ("lan.vera_outcome.kind", "real"),
        ("lan.startup.map", expected_map),
        ("lan.vmce.map_file", expected_map),
        ("lan.vmce.host", is_host),
    ]
    for path, expected in expectations:
        try:
            value = receipt_field(receipt, path)
        except KeyError:
            return Check(f"identity {label}", False, f"missing {path}")
        if _normalise(value) != _normalise(expected):
            return Check(
                f"identity {label}",
                False,
                f"{path}: expected {format_value(expected)}, got {format_value(value)}",
            )
    try:
        failure = receipt_field(receipt, "lan.failure")
    except KeyError:
        failure = None
    if failure is not None:
        return Check(f"identity {label}", False, f"lan.failure = {format_value(failure)}")
    return Check(f"identity {label}", True, f"map={expected_map}")


def _participant_count_check(
    receipts: Sequence[tuple[str, Mapping[str, Any]]], players: int
) -> Check:
    for label, receipt in receipts:
        path = "lan.configured_participant_count"
        try:
            value = receipt_field(receipt, path)
        except KeyError:
            return Check("configured participants", False, f"missing {path} in {label}")
        if value != players:
            return Check(
                "configured participants",
                False,
                f"{path}: expected {players}, got {format_value(value)} in {label}",
            )
    return Check("configured participants", True, f"{players} rows")


def _cleanup_process_dirs(specs: Sequence[ProcessSpec]) -> list[str]:
    errors: list[str] = []
    for spec in specs:
        for path in (spec.retail, spec.settings):
            if not path.exists():
                continue
            try:
                shutil.rmtree(path, onexc=_force_remove)
            except OSError as exc:
                errors.append(f"{path}: {exc}")
    return errors


def _force_remove(function: Any, path: str, error: BaseException) -> None:
    """Make read-only retail files removable on Windows."""
    try:
        os.chmod(path, stat.S_IWRITE)
        function(path)
    except OSError:
        raise error


def run_scenario(context: RunContext, scenario: Scenario, index: int, total: int) -> ScenarioResult:
    """Run one scenario to completion and return its checks."""
    validate_scenario_checkpoints(scenario)
    players = validate_players(scenario.players if scenario.players is not None else context.players)
    print(f"== {scenario.name} ({players} players) ==", flush=True)

    ports = process_ports(context.port_base, players)
    for port in ports:
        for probe_port in (port, port + 2):
            probe_udp_free("127.0.0.1", probe_port)
    binds = [f"127.0.0.1:{port}" for port in ports]
    specs: list[ProcessSpec] = []
    for process_index, bind in enumerate(binds):
        peers = binds[1:] if process_index == 0 else (binds[0],)
        specs.append(
            _prepare_process(
                context,
                scenario,
                process_index,
                bind,
                peers,
                players,
            )
        )

    environment = _process_environment()
    launched: list[LaunchedProcess] = []
    checks: list[Check] = []
    error: str | None = None
    marker_seen = False
    try:
        try:
            host = _launch_process(specs[0], environment)
            launched.append(host)
            # The co-op campaign host only advertises its room after reading
            # the campaign; launching the guest immediately gives the slower
            # start enough overlap with the host's first admission wait.
            # The classic co-op host needs the guest early; the VMCE host only
            # advertises its room after reading the campaign, so wait for it.
            immediate_guest = (
                scenario.expect_retry
                and scenario.checkpoint_host == "lan-live-coop-host"
            )
            marker_seen = False if immediate_guest else wait_for_host_ready(host)
            if host.popen.poll() is not None:
                raise RunFailure(
                    f"host exited with code {host.popen.returncode} before the guests "
                    f"were started; see {host.spec.stderr_path}"
                )
            for spec in specs[1:]:
                launched.append(_launch_process(spec, environment))
            wait_error = wait_for_scenario(launched, context.timeout_seconds)
            if wait_error is not None:
                raise RunFailure(wait_error)
        except RunFailure as exc:
            error = str(exc)
            for process in launched:
                kill_process_tree(process)
        finally:
            for process in launched:
                process.close()
    except BaseException:
        for process in launched:
            kill_process_tree(process)
            process.close()
        raise

    process_records = [_process_record(process) for process in launched]
    for process in launched:
        checks.append(
            Check(
                f"process {process.spec.role}",
                process.popen.returncode == 0 and not process.timed_out,
                f"exit={process.popen.returncode} timed_out={process.timed_out}",
            )
        )
    if error is not None:
        checks.append(Check("run", False, error))

    receipts: list[tuple[str, Mapping[str, Any]]] = []
    if error is None:
        for process in launched:
            receipt_path = process.spec.output / "capture.json"
            try:
                receipt = _load_receipt(receipt_path)
            except RunFailure as exc:
                checks.append(Check(f"receipt {process.spec.role}", False, str(exc)))
                error = str(exc)
                continue
            receipts.append((process.spec.role, receipt))
            checks.append(Check(f"receipt {process.spec.role}", True, str(receipt_path)))
        if error is None and len(receipts) == players:
            checks.extend(
                evaluate_receipts(
                    receipts,
                    players=players,
                    expect_victory=scenario.expect_victory,
                    expect_can_continue=scenario.expect_can_continue,
                    expect_rows=players,
                    expect_next_stage=scenario.expect_next_stage,
                    expect_retry=scenario.expect_retry,
                    expect_screenshots=scenario.expect_screenshots,
                )
            )
            for process in launched:
                receipt = dict(receipts)[process.spec.role]
                checkpoint = (
                    scenario.checkpoint_host
                    if process.spec.index == 0
                    else scenario.checkpoint_guest
                )
                if scenario.expect_retry:
                    checks.append(
                        _retry_identity_check(
                            process.spec.role,
                            receipt,
                            checkpoint,
                            process.spec.bind,
                            context.map_name,
                        )
                    )
                    continue
                checks.append(
                    _receipt_identity_check(
                        process.spec.role,
                        receipt,
                        checkpoint,
                        process.spec.bind,
                        context.map_name,
                        process.spec.index == 0,
                    )
                )
                checks.append(
                    Check(
                        f"next stage {process.spec.role}",
                        _next_stage_loaded(receipt),
                        "lan.vera_outcome.next_mission present"
                        if _next_stage_loaded(receipt)
                        else "missing lan.vera_outcome.next_mission",
                    )
                )
            if not scenario.expect_retry:
                checks.append(_participant_count_check(receipts, players))
        else:
            checks.append(
                Check(
                    "receipts",
                    False,
                    f"loaded {len(receipts)} of {players} capture receipts",
                )
            )

    result = ScenarioResult(
        name=scenario.name,
        players=players,
        checks=checks,
        processes=process_records,
        error=error,
        host_room_marker=marker_seen,
        specs=specs,
    )
    print_checks(checks)
    print(f"scenario {scenario.name}: {'PASS' if result.passed else 'FAIL'}")
    if result.error:
        print(f"  reason: {result.error}")
    print(f"[{index + 1}/{total}] {scenario.name} complete", flush=True)
    return result


def _next_stage_loaded(receipt: Mapping[str, Any]) -> bool:
    try:
        next_mission = receipt_field(receipt, "lan.vera_outcome.next_mission")
    except KeyError:
        return False
    return isinstance(next_mission, Mapping) and _is_str(next_mission.get("map"))


def probe_udp_free(host: str, port: int) -> None:
    """Bind a UDP probe and fail with the exact conflicting address."""
    probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        probe.bind((host, port))
    except OSError as exc:
        raise SetupError(f"required UDP address {host}:{port} is not free: {exc}") from exc
    finally:
        probe.close()


# ---------------------------------------------------------------------------
# CLI commands
# ---------------------------------------------------------------------------


def print_checks(checks: Sequence[Check]) -> None:
    for check in checks:
        status = "PASS" if check.ok else "FAIL"
        suffix = f": {check.detail}" if check.detail else ""
        print(f"  {status}  {check.name}{suffix}")


def _validate_run_args(args: argparse.Namespace) -> RunContext:
    validate_players(args.players)
    validate_jobs(args.jobs)
    timeout = validate_timeout_seconds(args.timeout_seconds)
    # A dev probe is a single short flow test: fail the capture fast unless
    # the caller explicitly chose a capture timeout.
    if args.suite == "dev" and "VMCE_CAPTURE_TIMEOUT" not in os.environ:
        global CAPTURE_TIMEOUT_SECONDS
        CAPTURE_TIMEOUT_SECONDS = 120
    validate_port_base(args.port_base, args.players)

    exe = Path(args.exe).resolve()
    if not exe.is_file():
        raise UsageError(f"--exe is not a file: {exe}")
    retail_source = Path(args.retail_source).resolve()
    if not retail_source.is_dir():
        raise UsageError(f"--retail-source is not a directory: {retail_source}")
    series = Path(args.series).resolve()
    if not series.is_dir():
        raise UsageError(f"--series is not a directory: {series}")
    if not (series / "campaign.ini").is_file():
        raise UsageError(f"--series has no campaign.ini: {series}")
    entry = validate_entry(series, args.entry)
    map_name = build_lan_map_name(series, entry)

    work_dir = Path(args.work_dir) if args.work_dir else default_work_dir()
    work_dir = work_dir.resolve()
    if work_dir.exists():
        if not work_dir.is_dir():
            raise UsageError(f"--work-dir is not a directory: {work_dir}")
        if any(work_dir.iterdir()):
            raise UsageError(f"--work-dir already exists and is not empty: {work_dir}")
    work_dir.mkdir(parents=True, exist_ok=True)

    return RunContext(
        exe=exe,
        retail_source=retail_source,
        series=series,
        series_name=series.name,
        entry=entry,
        map_name=map_name,
        work_dir=work_dir,
        port_base=args.port_base,
        timeout_seconds=timeout,
        keep=args.keep,
        players=args.players,
        suite=args.suite,
    )


def default_work_dir() -> Path:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return Path(".scratch") / "vmce-lan-acceptance" / f"{stamp}-{os.getpid()}"


def write_summary(path: Path, summary: Mapping[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    os.replace(temporary, path)


def _summary_document(
    context: RunContext,
    scenarios: Sequence[Scenario],
    results: Sequence[ScenarioResult],
) -> dict[str, Any]:
    return {
        "schema": "vera20k.vmce-lan-acceptance.v1",
        "started_at": STARTED_AT,
        "work_dir": str(context.work_dir),
        "suite": context.suite,
        "players": context.players,
        "exe": str(context.exe),
        "retail_source": str(context.retail_source),
        "series": str(context.series),
        "series_name": context.series_name,
        "entry": context.entry,
        "map": context.map_name,
        "port_base": context.port_base,
        "timeout_seconds": context.timeout_seconds,
        "keep": context.keep,
        "scenarios_requested": [scenario.name for scenario in scenarios],
        "scenarios": [result.as_json() for result in results],
        "status": "pass"
        if len(results) == len(scenarios) and all(result.passed for result in results)
        else "fail",
    }


def run_command(args: argparse.Namespace) -> int:
    """Implement ``run``; returns the process exit status."""
    scenarios = selected_scenarios(args)
    if args.suite == "full" and not args.scenario:
        print("note: the full suite equals smoke in v1; defeat/final/houses are reserved")
    context = _validate_run_args(args)
    summary_path = context.work_dir / "summary.json"
    summary = _summary_document(context, scenarios, [])
    write_summary(summary_path, summary)
    print(f"work dir: {context.work_dir}")
    print(f"map: {context.map_name}")

    results: list[ScenarioResult] = []
    try:
        for index, scenario in enumerate(scenarios):
            result = run_scenario(context, scenario, index, len(scenarios))
            results.append(result)
            summary = _summary_document(context, scenarios, results)
            write_summary(summary_path, summary)
            if not result.passed:
                print(f"failure paths kept under {context.work_dir / scenario.name}")
    except (SetupError, OSError) as exc:
        summary = _summary_document(context, scenarios, results)
        summary["status"] = "setup-error"
        summary["error"] = str(exc)
        write_summary(summary_path, summary)
        print(f"work dir kept: {context.work_dir}", file=sys.stderr)
        raise

    passed = len(results) == len(scenarios) and all(result.passed for result in results)
    if passed and not context.keep:
        cleanup_errors = [
            error
            for result in results
            for error in _cleanup_process_dirs(result.specs)
        ]
        for result in results:
            result.cleaned = not cleanup_errors
        if cleanup_errors:
            print("warning: some settings/retail copies could not be removed:", file=sys.stderr)
            for cleanup_error in cleanup_errors:
                print(f"  {cleanup_error}", file=sys.stderr)
        summary = _summary_document(context, scenarios, results)
        write_summary(summary_path, summary)

    print()
    print(f"summary: {summary_path}")
    if not passed:
        print(f"kept run directory: {context.work_dir}")
        for result in results:
            print(f"  {result.name}: {context.work_dir / result.name}")
    elif context.keep:
        print("per-process settings/retail copies kept (--keep)")
    else:
        print("per-process settings/retail copies removed; outputs and logs kept")
    print(f"RESULT: {'PASS' if passed else 'FAIL'} ({len(results)}/{len(scenarios)} scenarios)")
    return 0 if passed else 1


def _load_receipts_for_compare(
    paths: Sequence[Path],
) -> tuple[list[tuple[str, Mapping[str, Any], Path]], list[Check]]:
    loaded: list[tuple[str, Mapping[str, Any], Path]] = []
    checks: list[Check] = []
    for index, path in enumerate(paths):
        label = "host" if index == 0 else f"guest{index}"
        try:
            receipt = _load_receipt(path)
        except RunFailure as exc:
            checks.append(Check(f"receipt {label}", False, str(exc)))
            continue
        loaded.append((label, receipt, path))
        checks.append(Check(f"receipt {label}", True, str(path)))
    return loaded, checks


def compare_command(args: argparse.Namespace) -> int:
    """Implement ``compare``; returns 0 for all checks, 1 otherwise."""
    players = validate_players(args.players)
    if args.expect_rows is not None and args.expect_rows < 0:
        raise UsageError(f"--expect-rows must not be negative, got {args.expect_rows}")
    if len(args.receipt) != players:
        raise UsageError(
            f"--players {players} needs {players} --receipt values in host-first order; "
            f"got {len(args.receipt)}"
        )
    loaded, checks = _load_receipts_for_compare(args.receipt)
    if len(loaded) != players:
        print_checks(checks)
        print("RESULT: FAIL")
        return 1
    checks.extend(
        evaluate_receipts(
            [(label, receipt) for label, receipt, _ in loaded],
            players=players,
            expect_victory=True if args.expect_victory else None,
            expect_can_continue=True if args.expect_can_continue else None,
            expect_rows=args.expect_rows,
            expect_next_stage=args.expect_next_stage,
            expect_retry=args.expect_retry,
            expect_screenshots=args.expect_screenshot,
            allow_missing_simulation_hashes=args.allow_missing_simulation_hashes,
        )
    )
    print_checks(checks)
    passed = all(check.ok for check in checks)
    print(f"RESULT: {'PASS' if passed else 'FAIL'}")
    return 0 if passed else 1


# ---------------------------------------------------------------------------
# argparse
# ---------------------------------------------------------------------------


def _positive_int(text: str) -> int:
    try:
        value = int(text)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"expected an integer, got {text!r}") from exc
    if value <= 0:
        raise argparse.ArgumentTypeError(f"expected a positive integer, got {value}")
    return value


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m tools.vmce_lan_acceptance",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    run_parser = subparsers.add_parser(
        "run",
        help="run VMCE LAN live-capture acceptance scenarios",
        description="Run VMCE LAN live-capture acceptance scenarios.",
    )
    run_parser.add_argument("--suite", choices=tuple(SUITES), default="fast")
    run_parser.add_argument(
        "--scenario",
        action="append",
        default=[],
        metavar="NAME",
        help="run a named built-in scenario instead of --suite (repeatable)",
    )
    run_parser.add_argument("--exe", required=True, help="VERA20k executable")
    run_parser.add_argument(
        "--retail-source",
        required=True,
        help="clean retail directory copied once per process",
    )
    run_parser.add_argument(
        "--series",
        required=True,
        help="VMCE series directory containing campaign.ini and stage maps",
    )
    run_parser.add_argument(
        "--entry",
        default=DEFAULT_ENTRY,
        help=f"map entry relative to --series (default: {DEFAULT_ENTRY})",
    )
    run_parser.add_argument(
        "--work-dir",
        default=None,
        help="run directory (default: .scratch/vmce-lan-acceptance/<utc>-<pid>)",
    )
    run_parser.add_argument("--players", type=int, default=4, choices=(2, 4))
    run_parser.add_argument(
        "--keep",
        action="store_true",
        help="keep copied settings/retail trees after a passing scenario",
    )
    run_parser.add_argument("--timeout-seconds", type=_positive_int, default=600)
    run_parser.add_argument(
        "--jobs",
        type=_positive_int,
        default=1,
        help="reserved: only 1 is supported in v1",
    )
    run_parser.add_argument(
        "--port-base",
        type=_positive_int,
        default=DEFAULT_PORT_BASE,
        help=f"first process UDP port (default: {DEFAULT_PORT_BASE})",
    )

    compare_parser = subparsers.add_parser(
        "compare",
        help="compare existing VMCE LAN capture receipts offline",
        description="Compare existing VMCE LAN capture receipts offline.",
    )
    compare_parser.add_argument("--players", type=int, required=True, choices=(2, 4))
    compare_parser.add_argument(
        "--receipt",
        action="append",
        type=Path,
        required=True,
        metavar="PATH",
        help="capture.json in host-first order (repeat once per player)",
    )
    compare_parser.add_argument("--expect-victory", action="store_true")
    compare_parser.add_argument(
        "--expect-retry",
        action="store_true",
        help="compare a real two-process defeat/retry capture",
    )
    compare_parser.add_argument("--expect-can-continue", action="store_true")
    compare_parser.add_argument("--expect-rows", type=int)
    compare_parser.add_argument("--expect-next-stage", action="store_true")
    compare_parser.add_argument(
        "--expect-screenshot",
        action="append",
        default=[],
        metavar="NAME",
        help="require this screenshot name in every receipt",
    )
    compare_parser.add_argument(
        "--allow-missing-simulation-hashes",
        action="store_true",
        help="accept receipts with absent/empty lan.simulation_hashes",
    )
    return parser


STARTED_AT = datetime.now(timezone.utc).isoformat(timespec="seconds")


def main(argv: Sequence[str] | None = None) -> int:
    """Entry point; ``run`` is the default subcommand."""
    arguments = list(sys.argv[1:] if argv is None else argv)
    if not arguments or arguments[0] not in ("run", "compare"):
        if any(argument in ("-h", "--help") for argument in arguments):
            build_parser().print_help()
            return 0
        arguments.insert(0, "run")
    parser = build_parser()
    args = parser.parse_args(arguments)
    try:
        if args.command == "compare":
            return compare_command(args)
        return run_command(args)
    except UsageError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    except SetupError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    except RunFailure as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    except OSError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
