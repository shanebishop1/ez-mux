#!/usr/bin/env python3
"""Static workflow contracts for release and CI verification boundaries."""

from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class WorkflowContractTests(unittest.TestCase):
    def test_ci_requires_complete_locked_surface_and_runtime_integration(self) -> None:
        workflow = (ROOT / ".github/workflows/ci-quality-gate.yml").read_text(encoding="utf-8")
        self.assertIn("run: cargo test --locked\n", workflow)
        self.assertIn("--test session_runtime_integration", workflow)
        self.assertIn("required-verification:", workflow)
        self.assertIn("if: always()", workflow)
        self.assertIn("fail-fast: false", workflow)

    def test_release_has_independent_e2e_matrix_and_all_required_results(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("e2e:", workflow)
        self.assertIn("fail-fast: false", workflow)
        self.assertIn("suite: foundation", workflow)
        self.assertIn("suite: core-session", workflow)
        self.assertIn("suite: smoke", workflow)
        self.assertIn("suite: reduced-layout", workflow)
        self.assertIn("suite: zoomed-mode", workflow)
        self.assertIn("run: cargo test --locked\n", workflow)
        self.assertIn("--test session_runtime_integration", workflow)
        self.assertIn("${{ needs.e2e.result }}", workflow)
        self.assertIn("--workflow-results dist/workflow-results.json", workflow)

    def test_native_verification_uses_downloaded_publishable_archive(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("Download exact publishable native archive", workflow)
        self.assertIn("run-native-release-verification.sh \"${{ matrix.platform }}\" \"$archive\"", workflow)
        self.assertNotIn("run-native-release-verification.sh linux target/release/ezm", workflow)
        self.assertNotIn("run-native-release-verification.sh macos target/release/ezm", workflow)
        self.assertIn("--native-records dist/native-records", workflow)
        self.assertIn("--release-archives dist/release-archives", workflow)
        self.assertNotIn("cp dist/native-records/*.json", workflow)

    def test_evidence_is_assembled_before_gate_and_publication_uses_bundle(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        assemble = workflow.index("name: Assemble release evidence bundle")
        evaluate = workflow.index("name: Evaluate release evidence gate")
        self.assertLess(assemble, evaluate)
        self.assertIn("Extract evaluated release evidence bundle", workflow)
        self.assertIn("--evidence-manifest", workflow)
        self.assertIn("artifacts/release-assets", workflow)

    def test_failed_gate_fallback_uses_a_valid_heredoc_terminator(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        fallback = workflow.index("name: Ensure a failed gate decision exists")
        archive = workflow.index("name: Archive assembled evidence")
        step = workflow[fallback:archive]
        self.assertIn("          python3 - <<'PY'\n          import json", step)
        self.assertIn("\n          PY\n", step)

    def test_npm_publication_requires_github_release_and_evidence(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("- release\n", workflow)
        self.assertIn("needs.release.result == 'success'", workflow)
        self.assertIn("validate_verification_metadata.py", workflow)
        self.assertLess(workflow.index("name: Create GitHub Release"), workflow.index("name: Publish npm package"))
        self.assertIn("already exists with the requested matching version", workflow)


if __name__ == "__main__":
    unittest.main()
