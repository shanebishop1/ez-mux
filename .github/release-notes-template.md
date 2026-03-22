Automated release artifacts for `__TAG__`.

- Tag: `__TAG__`
- Commit SHA: `__COMMIT_SHA__`
- Workflow run: __WORKFLOW_RUN_URL__

## Artifacts

- `ezm-__TAG__-x86_64-unknown-linux-gnu.tar.gz`
- `ezm-__TAG__-x86_64-apple-darwin.tar.gz`
- `ezm-__TAG__-aarch64-apple-darwin.tar.gz`
- `ezm-__TAG__-checksums.txt`
- `ezm-__TAG__-sbom-status.txt` (status: `__SBOM_STATUS__`)

## Verify checksums

```bash
# Linux
sha256sum --check "ezm-__TAG__-checksums.txt"

# macOS
shasum -a 256 --check "ezm-__TAG__-checksums.txt"
```

## Collaborator feedback and install checklist

Install guide: __COLLAB_INSTALL_DOC_URL__

- [ ] Install the release archive for your platform.
- [ ] Confirm `ezm --version` reports `__TAG__`.
- [ ] Run your normal smoke test flow and report issues.
- [ ] Note any packaging/install friction by OS + architecture.
