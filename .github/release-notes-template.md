Automated release artifacts for `__TAG__`.

- Tag: `__TAG__`
- Package version: `__VERSION__`
- Commit SHA: `__COMMIT_SHA__`
- Workflow run: __WORKFLOW_RUN_URL__

## Artifacts

- `ezm-__TAG__-linux-x64.tar.gz`
- `ezm-__TAG__-linux-arm64.tar.gz`
- `ezm-__TAG__-macos-x64.tar.gz`
- `ezm-__TAG__-macos-arm64.tar.gz`
- `ezm-__TAG__-checksums.txt`
- `ezm-__TAG__-sbom.spdx.json` (status: `__SBOM_STATUS__`)
- `ezm-__TAG__-sbom-status.txt`
- `ezm-__TAG__-verification.json`
- `ezm-__TAG__-release-evidence.tar.gz`

The verification JSON records the enforced quality, MSRV, native install/version,
and Linux/macOS tmux E2E checks. The evidence archive contains the assembled
machine-readable gate inputs and decision. Native runtime smoke checks execute
only on the matching Linux and macOS runners; cross-compiled archives are
checked for safe contents and permissions but are not claimed to have run.

## Verify checksums

```bash
# Linux
sha256sum --check "ezm-__TAG__-checksums.txt"

# macOS
shasum -a 256 --check "ezm-__TAG__-checksums.txt"
```
