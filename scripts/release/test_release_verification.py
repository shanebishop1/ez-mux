#!/usr/bin/env python3
"""Focused tests for release input and archive verification boundaries."""

from __future__ import annotations

import importlib.util
import json
import os
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent


def load_script(name: str):
    spec = importlib.util.spec_from_file_location(name, SCRIPT_DIR / f"{name}.py")
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {name}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


package_release_archive = load_script("package_release_archive")
validate_release_ref = load_script("validate_release_ref")
verify_release_artifact = load_script("verify_release_artifact")
release_evidence = load_script("release_evidence")
release_gate = load_script("release_gate")


class ReleaseVerificationTests(unittest.TestCase):
    WORKFLOW_COMMIT_SHA = "workflow-commit"

    def _write_evaluated_manifest(self, root: Path) -> Path:
        bundle = root / "release-0.2.30"
        artifacts = bundle / "artifacts"
        artifacts.mkdir(parents=True)
        records = []

        def add(relative: str, content: object, category: str, suite: str, platform: str) -> dict:
            path = bundle / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(content, str):
                path.write_text(content, encoding="utf-8")
            else:
                path.write_bytes(content)
            record = {
                "suite": suite,
                "run_id": f"{suite}-{platform}",
                "platform": platform,
                "category": category,
                "path": relative,
                "sha256": release_evidence.sha256_file(path),
                "size_bytes": path.stat().st_size,
                "source_path": relative,
            }
            records.append(record)
            return record

        def case(case_id: str, *, core_schema: bool) -> dict:
            evidence = {
                "id": case_id,
                "pass": True,
                "assertions": [f"{case_id} passed"],
                "samples": [],
                "settle": {
                    "attempts": 1,
                    "poll_interval_ms": 50,
                    "timeout_ms": 2000,
                    "stable": True,
                    "sessions": "",
                    "windows": "",
                    "panes": "",
                },
            }
            if core_schema:
                evidence.update(
                    {
                        "snapshot": {"name": "fixture", "exists": True, "count": 1},
                        "layout": None,
                        "slots": None,
                        "remote_path": None,
                        "helper_state": None,
                    }
                )
            return evidence

        for suite, case_ids in release_evidence.EXPECTED_CASE_IDS_BY_SUITE.items():
            for platform in release_evidence.REQUIRED_OS:
                suffix = "" if platform == "linux" else "/macos"
                core_schema = suite != "foundation"
                cases = [case(case_id, core_schema=core_schema) for case_id in case_ids]
                metadata = {
                    "run_id": f"{suite}-{platform}",
                    "commit_sha": self.WORKFLOW_COMMIT_SHA,
                    "os": platform,
                    "test_ids": list(case_ids),
                    "pass_total": len(cases),
                    "fail_total": 0,
                }
                prefix = f"artifacts/{suite}{suffix}"
                add(
                    f"{prefix}/summary.json",
                    json.dumps({"metadata": metadata, "cases": cases}),
                    "machine-readable-results",
                    suite,
                    platform,
                )
                for case_data in cases:
                    category = (
                        "width-evidence"
                        if case_data["id"] in {"E2E-02", "E2E-12", "E2E-13"}
                        else "tmux-snapshots"
                    )
                    add(
                        f"{prefix}/cases/{case_data['id']}.json",
                        json.dumps(case_data),
                        category,
                        suite,
                        platform,
                    )
                if suite == "cross-platform-smoke":
                    add(f"{prefix}/envelope.json", "{}", "machine-readable-results", suite, platform)
                    add(
                        f"{prefix}/matrix.json",
                        json.dumps({"profile": "core-flows-v1", "platform": platform, "test_ids": list(case_ids)}),
                        "machine-readable-results",
                        suite,
                        platform,
                    )
                    add(
                        f"{prefix}/topology.json",
                        json.dumps({"run_id": f"{suite}-{platform}", "cases": []}),
                        "tmux-snapshots",
                        suite,
                        platform,
                    )

        for platform in ("linux", "macos"):
            prefix = "artifacts/install-validation" if platform == "linux" else "artifacts/install-validation/macos"
            install_metadata = {"os": platform, "test_ids": ["E2E-00"], "pass_total": 1, "fail_total": 0}
            add(f"{prefix}/summary.json", json.dumps({"platform": platform, "status": "passed", "metadata": install_metadata}), "machine-readable-results", "install-validation", platform)
            add(f"{prefix}/envelope.json", "{}", "machine-readable-results", "install-validation", platform)
            add(f"{prefix}/contract-smoke/help.txt", "Usage: ezm", "run-metadata", "install-validation", platform)
            add(f"{prefix}/contract-smoke/version.txt", "ezm 0.2.30", "run-metadata", "install-validation", platform)

        native_entries = []
        for platform, asset in (("linux", "linux-x64"), ("macos", "macos-x64")):
            archive_relative = f"artifacts/release-assets/ezm-v0.2.30-{asset}.tar.gz"
            archive = add(archive_relative, f"archive-{asset}", "release-assets", "release-assets", platform)
            archive["asset"] = asset
            native_relative = f"artifacts/native-verification/{platform}.json"
            native = {
                "platform": platform,
                "status": "passed",
                "archive": {"path": archive_relative, "archive_sha256": archive["sha256"], "member_sha256": "a" * 64},
            }
            native_record = add(native_relative, json.dumps(native), "native-verification", "native-release", platform)
            native_entries.append({"platform": platform, "path": native_relative, "sha256": native_record["sha256"]})
        for platform, asset in (("linux", "linux-arm64"), ("macos", "macos-arm64")):
            archive = add(f"artifacts/release-assets/ezm-v0.2.30-{asset}.tar.gz", f"archive-{asset}", "release-assets", "release-assets", platform)
            archive["asset"] = asset

        run_metadata = [
            {
                "suite": suite,
                "run_id": f"{suite}-{platform}",
                "platform": platform,
                "os": platform,
                "commit_sha": self.WORKFLOW_COMMIT_SHA,
                "test_ids": list(
                    release_evidence.EXPECTED_CASE_IDS_BY_SUITE.get(suite, ("E2E-00",))
                ),
                "pass_total": len(release_evidence.EXPECTED_CASE_IDS_BY_SUITE.get(suite, ("E2E-00",))),
                "fail_total": 0,
            }
            for suite in release_evidence.REQUIRED_RELEASE_SUITES
            for platform in release_evidence.REQUIRED_OS
        ]
        paths_by_category = {}
        for record in records:
            paths_by_category.setdefault(record["category"], []).append(record["path"])
        manifest = {
            "schema_version": release_evidence.SCHEMA_VERSION,
            "bundle_id": "release-0.2.30",
            "artifacts": records,
            "evidence_index": {
                "machine_readable_results": paths_by_category["machine-readable-results"],
                "tmux_structure_snapshots": paths_by_category["tmux-snapshots"],
                "pane_width_evidence": paths_by_category["width-evidence"],
                "run_metadata": [f"{entry['suite']}:{entry['platform']}" for entry in run_metadata],
                "install_validation_summaries": [record["path"] for record in records if record["suite"] == "install-validation" and record["path"].endswith("/summary.json")],
                "native_verification": [entry["path"] for entry in native_entries],
                "release_assets": paths_by_category["release-assets"],
            },
            "run_metadata": run_metadata,
            "native_verification": native_entries,
        }
        manifest_path = bundle / "manifest.json"
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return manifest_path

    def _write_workflow_results(self, root: Path, commit_sha: str | None = None) -> Path:
        workflow_results = root / "workflow-results.json"
        workflow_results.write_text(
            json.dumps(
                {
                    "inputs": {
                        "tag": "v0.2.30",
                        "version": "0.2.30",
                        "commit_sha": commit_sha or self.WORKFLOW_COMMIT_SHA,
                    },
                    "jobs": {job: "success" for job in release_evidence.REQUIRED_WORKFLOW_JOBS},
                }
            ),
            encoding="utf-8",
        )
        return workflow_results

    def _rewrite_manifest(self, manifest: Path, mutate) -> None:
        data = json.loads(manifest.read_text(encoding="utf-8"))
        mutate(data)
        for record in data["artifacts"]:
            artifact_path = manifest.parent / record["path"]
            if artifact_path.is_file():
                record["sha256"] = release_evidence.sha256_file(artifact_path)
                record["size_bytes"] = artifact_path.stat().st_size
        manifest.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    def _decision_codes(self, manifest: Path) -> set[str]:
        workflow_results = self._write_workflow_results(manifest.parent)
        return {
            item["code"]
            for item in release_gate.evaluate_release_gate(manifest, workflow_results)["blocking_reasons"]
        }

    def test_realistic_current_e2e_fixture_passes(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))
            workflow_results = self._write_workflow_results(manifest.parent)
            decision = release_gate.evaluate_release_gate(manifest, workflow_results)
            self.assertTrue(decision["passed"], decision["blocking_reasons"])
            self.assertEqual(
                release_evidence.FULL_REGRESSION_IDS,
                tuple(
                    [f"E2E-{index:02d}" for index in range(14)]
                    + ["E2E-15", "E2E-16", "E2E-17", "E2E-18", "E2E-19", "E2E-20", "E2E-21"]
                ),
            )

    def test_e2e_commits_must_match_validated_workflow_commit(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))
            workflow_results = self._write_workflow_results(manifest.parent, "different-commit")
            decision = release_gate.evaluate_release_gate(manifest, workflow_results)
            self.assertFalse(decision["passed"])
            self.assertIn("e2e-commit-sha-mismatch", {item["code"] for item in decision["blocking_reasons"]})

    def test_missing_e2e_summary_or_run_commit_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))

            def remove_commits(data):
                summary_path = manifest.parent / "artifacts/foundation/summary.json"
                summary = json.loads(summary_path.read_text(encoding="utf-8"))
                summary["metadata"].pop("commit_sha")
                summary_path.write_text(json.dumps(summary), encoding="utf-8")
                for entry in data["run_metadata"]:
                    if entry.get("suite") == "core-session-orchestration" and entry.get("platform") == "macos":
                        entry.pop("commit_sha")
                        break

            self._rewrite_manifest(manifest, remove_commits)
            codes = self._decision_codes(manifest)
            self.assertIn("e2e-summary-commit-sha-missing", codes)
            self.assertIn("e2e-run-commit-sha-missing", codes)

    def test_standalone_case_must_equal_summary_case(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))

            def contradict_case(_data):
                case_path = manifest.parent / "artifacts/cross-platform-smoke/cases/E2E-01.json"
                case_data = json.loads(case_path.read_text(encoding="utf-8"))
                case_data["assertions"].append("contradictory standalone evidence")
                case_path.write_text(json.dumps(case_data), encoding="utf-8")

            self._rewrite_manifest(manifest, contradict_case)
            self.assertIn("e2e-case-artifact-invalid", self._decision_codes(manifest))

    def test_mutated_native_verification_record_fails_evaluation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))
            workflow_results = self._write_workflow_results(manifest.parent)
            self.assertTrue(release_gate.evaluate_release_gate(manifest, workflow_results)["passed"])
            native_path = manifest.parent / "artifacts/native-verification/linux.json"
            native_path.write_text(native_path.read_text(encoding="utf-8").replace('"passed"', '"failed"'), encoding="utf-8")
            decision = release_gate.evaluate_release_gate(manifest, workflow_results)
            self.assertFalse(decision["passed"])
            self.assertIn("manifest-artifact-hash-mismatch", {item["code"] for item in decision["blocking_reasons"]})

    def test_omitted_native_verification_record_fails_evaluation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))
            workflow_results = self._write_workflow_results(manifest.parent)
            self.assertTrue(release_gate.evaluate_release_gate(manifest, workflow_results)["passed"])
            data = json.loads(manifest.read_text(encoding="utf-8"))
            data["artifacts"] = [record for record in data["artifacts"] if record["category"] != "native-verification" or record["platform"] != "macos"]
            data["native_verification"] = [entry for entry in data["native_verification"] if entry["platform"] != "macos"]
            data["evidence_index"]["native_verification"] = [path for path in data["evidence_index"]["native_verification"] if not path.endswith("/macos.json")]
            manifest.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            decision = release_gate.evaluate_release_gate(manifest, workflow_results)
            self.assertFalse(decision["passed"])
            self.assertIn("native-verification-records-missing", {item["code"] for item in decision["blocking_reasons"]})

    def test_incomplete_summary_cases_fail_evaluation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))

            def remove_case(data):
                data["artifacts"] = [
                    record
                    for record in data["artifacts"]
                    if record["path"] != "artifacts/core-session-orchestration/cases/E2E-21.json"
                ]

            manifest.parent.joinpath("artifacts/core-session-orchestration/cases/E2E-21.json").unlink()
            self._rewrite_manifest(manifest, remove_case)
            codes = self._decision_codes(manifest)
            self.assertIn("e2e-case-artifacts-incomplete", codes)
            self.assertIn("carry-forward-missing-linux", codes)

    def test_lying_summary_counts_fail_evaluation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))

            def lie(data):
                summary_path = manifest.parent / "artifacts/cross-platform-smoke/summary.json"
                summary = json.loads(summary_path.read_text(encoding="utf-8"))
                summary["metadata"]["pass_total"] = 1
                summary_path.write_text(json.dumps(summary), encoding="utf-8")

            self._rewrite_manifest(manifest, lie)
            self.assertIn("e2e-summary-counts-invalid", self._decision_codes(manifest))

    def test_failed_case_and_summary_fail_evaluation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))

            def fail_case(data):
                summary_path = manifest.parent / "artifacts/core-session-orchestration/summary.json"
                case_path = manifest.parent / "artifacts/core-session-orchestration/cases/E2E-20.json"
                summary = json.loads(summary_path.read_text(encoding="utf-8"))
                case_data = json.loads(case_path.read_text(encoding="utf-8"))
                case_data["pass"] = False
                for summary_case in summary["cases"]:
                    if summary_case["id"] == "E2E-20":
                        summary_case["pass"] = False
                summary["metadata"]["pass_total"] -= 1
                summary["metadata"]["fail_total"] = 1
                summary_path.write_text(json.dumps(summary), encoding="utf-8")
                case_path.write_text(json.dumps(case_data), encoding="utf-8")

            self._rewrite_manifest(manifest, fail_case)
            codes = self._decision_codes(manifest)
            self.assertIn("e2e-summary-counts-invalid", codes)
            self.assertIn("suite-failures-present", codes)

    def test_missing_new_core_cases_are_required(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))
            case_path = manifest.parent / "artifacts/core-session-orchestration/cases/E2E-20.json"
            case_path.unlink()

            def remove_case(data):
                data["artifacts"] = [record for record in data["artifacts"] if record["path"] != "artifacts/core-session-orchestration/cases/E2E-20.json"]

            self._rewrite_manifest(manifest, remove_case)
            self.assertIn("e2e-case-artifacts-incomplete", self._decision_codes(manifest))

    def test_release_tag_rejects_shell_source(self) -> None:
        self.assertIsNone(validate_release_ref.TAG_PATTERN.fullmatch("v0.2.30; touch pwned"))
        self.assertEqual(
            validate_release_ref.TAG_PATTERN.fullmatch("v0.2.30").group("version"),
            "0.2.30",
        )

    def test_native_archive_and_version_are_verified(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            root = Path(temp_dir)
            binary = root / "ezm"
            binary.write_text("#!/bin/sh\nprintf '%s\\n' 'ezm 0.2.30'\n", encoding="utf-8")
            binary.chmod(binary.stat().st_mode | stat.S_IXUSR)
            archive = package_release_archive.package_archive(
                binary, "v0.2.30", "0.2.30", "linux-x64", root / "dist"
            )

            result = verify_release_artifact.verify_release(
                binary, archive, "0.2.30", "linux"
            )

            self.assertEqual(result["status"], "passed")
            self.assertEqual(result["archive"]["member"], "ezm")
            self.assertEqual(
                result["archive"]["archive_sha256"],
                verify_release_artifact.sha256_file(archive),
            )
            archive_result = verify_release_artifact.verify_release_archive(archive, "0.2.30", "linux")
            self.assertEqual(archive_result["binary"]["source"], "archive member ezm")
            self.assertEqual(
                archive_result["binary"]["sha256"], result["binary"]["sha256"]
            )

    def test_archive_path_traversal_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            archive = Path(temp_dir) / "unsafe.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                info = tarfile.TarInfo("../pwned")
                info.mode = 0o755
                info.size = 0
                bundle.addfile(info)

            with self.assertRaises(SystemExit):
                verify_release_artifact.verify_archive(archive, "0.2.30")

    def test_binary_and_archive_identity_is_verified(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            root = Path(temp_dir)
            binary = root / "ezm"
            binary.write_text("#!/bin/sh\nprintf '%s\\n' 'ezm 0.2.30'\n", encoding="utf-8")
            binary.chmod(binary.stat().st_mode | stat.S_IXUSR)
            archive = package_release_archive.package_archive(
                binary, "v0.2.30", "0.2.30", "linux-x64", root / "dist"
            )
            binary.write_text("#!/bin/sh\nprintf '%s\\n' 'ezm 0.2.30 changed'\n", encoding="utf-8")
            binary.chmod(binary.stat().st_mode | stat.S_IXUSR)

            with self.assertRaises(SystemExit):
                verify_release_artifact.verify_release(binary, archive, "0.2.30", "linux")

    def test_failed_workflow_result_emits_failed_evidence(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            root = Path(temp_dir)
            workflow_results = root / "workflow-results.json"
            workflow_results.write_text(
                json.dumps(
                    {
                        "inputs": {"tag": "v0.2.30", "version": "0.2.30", "commit_sha": "abc"},
                        "jobs": {
                            "validate-ref": "success",
                            "quality-gate": "failure",
                            "locked-tests": "success",
                            "session-runtime-integration": "success",
                            "msrv": "success",
                            "e2e": "success",
                            "build": "success",
                            "native-release": "success",
                        },
                    }
                ),
                encoding="utf-8",
            )
            gate = root / "gate.json"
            gate.write_text(json.dumps({"passed": False}), encoding="utf-8")
            output = root / "verification.json"
            environment = {
                **os.environ,
                "RELEASE_TAG": "v0.2.30",
                "RELEASE_VERSION": "0.2.30",
                "RELEASE_SHA": "abc",
                "RELEASE_RUN_URL": "https://example.invalid/run/1",
            }
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT_DIR / "emit_verification_metadata.py"),
                    "--output",
                    str(output),
                    "--evidence-manifest",
                    str(root / "manifest.json"),
                    "--gate-decision",
                    str(gate),
                    "--workflow-results",
                    str(workflow_results),
                ],
                env=environment,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(json.loads(output.read_text(encoding="utf-8"))["status"], "failed")
            rejected = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT_DIR / "validate_verification_metadata.py"),
                     "--verification",
                     str(output),
                     "--evidence-manifest",
                     str(root / "manifest.json"),
                     "--gate-decision",
                    str(gate),
                    "--workflow-results",
                    str(workflow_results),
                    "--tag",
                    "v0.2.30",
                    "--version",
                    "0.2.30",
                    "--commit-sha",
                    "abc",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(rejected.returncode, 0)


if __name__ == "__main__":
    unittest.main()
