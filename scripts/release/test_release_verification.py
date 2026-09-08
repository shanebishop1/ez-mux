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
assemble_release_bundle = load_script("assemble_release_bundle")


class ReleaseVerificationTests(unittest.TestCase):
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
                "sha256": assemble_release_bundle.sha256_file(path),
                "size_bytes": path.stat().st_size,
                "source_path": relative,
            }
            records.append(record)
            return record

        all_ids = list(assemble_release_bundle.FULL_REGRESSION_IDS)
        metadata = {"os": "linux", "test_ids": all_ids, "pass_total": 20, "fail_total": 0}
        add("artifacts/foundation/summary.json", json.dumps({"metadata": metadata}), "machine-readable-results", "foundation", "linux")
        add("artifacts/core-session-orchestration/summary.json", json.dumps({"metadata": metadata}), "machine-readable-results", "core-session-orchestration", "linux")
        add("artifacts/core-session-orchestration/cases/E2E-12.json", json.dumps({"pass": True}), "width-evidence", "core-session-orchestration", "linux")
        add("artifacts/core-session-orchestration/cases/E2E-13.json", json.dumps({"pass": True}), "width-evidence", "core-session-orchestration", "linux")
        for name in ("summary.json", "envelope.json", "matrix.json", "topology.json"):
            category = "tmux-snapshots" if name == "topology.json" else "machine-readable-results"
            content = json.dumps({"metadata": metadata}) if name == "summary.json" else "{}"
            add(f"artifacts/cross-platform-smoke/{name}", content, category, "cross-platform-smoke", "linux")

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
            {"suite": suite, "run_id": f"{suite}-{platform}", "platform": platform, "os": platform, "test_ids": all_ids, "pass_total": 20, "fail_total": 0}
            for suite in assemble_release_bundle.REQUIRED_RELEASE_SUITES
            for platform in assemble_release_bundle.REQUIRED_OS
        ]
        paths_by_category = {}
        for record in records:
            paths_by_category.setdefault(record["category"], []).append(record["path"])
        manifest = {
            "schema_version": assemble_release_bundle.SCHEMA_VERSION,
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

    def test_mutated_native_verification_record_fails_evaluation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))
            self.assertTrue(assemble_release_bundle.evaluate_release_gate(manifest)["passed"])
            native_path = manifest.parent / "artifacts/native-verification/linux.json"
            native_path.write_text(native_path.read_text(encoding="utf-8").replace('"passed"', '"failed"'), encoding="utf-8")
            decision = assemble_release_bundle.evaluate_release_gate(manifest)
            self.assertFalse(decision["passed"])
            self.assertIn("manifest-artifact-hash-mismatch", {item["code"] for item in decision["blocking_reasons"]})

    def test_omitted_native_verification_record_fails_evaluation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ezm-release-test-") as temp_dir:
            manifest = self._write_evaluated_manifest(Path(temp_dir))
            self.assertTrue(assemble_release_bundle.evaluate_release_gate(manifest)["passed"])
            data = json.loads(manifest.read_text(encoding="utf-8"))
            data["artifacts"] = [record for record in data["artifacts"] if record["category"] != "native-verification" or record["platform"] != "macos"]
            data["native_verification"] = [entry for entry in data["native_verification"] if entry["platform"] != "macos"]
            data["evidence_index"]["native_verification"] = [path for path in data["evidence_index"]["native_verification"] if not path.endswith("/macos.json")]
            manifest.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            decision = assemble_release_bundle.evaluate_release_gate(manifest)
            self.assertFalse(decision["passed"])
            self.assertIn("native-verification-records-missing", {item["code"] for item in decision["blocking_reasons"]})

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
