Automated release artifacts for `__TAG__`.

- Tag: `__TAG__`
- Commit SHA: `__COMMIT_SHA__`
- Workflow run: __WORKFLOW_RUN_URL__

## Artifacts

- `ezm-__TAG__-linux-x64.tar.gz`
- `ezm-__TAG__-linux-arm64.tar.gz`
- `ezm-__TAG__-macos-x64.tar.gz`
- `ezm-__TAG__-macos-arm64.tar.gz`
- `ezm-__TAG__-checksums.txt`
- `ezm-__TAG__-sbom-status.txt` (status: `__SBOM_STATUS__`)

## Verify checksums

```bash
# Linux
sha256sum --check "ezm-__TAG__-checksums.txt"

# macOS
shasum -a 256 --check "ezm-__TAG__-checksums.txt"
```
