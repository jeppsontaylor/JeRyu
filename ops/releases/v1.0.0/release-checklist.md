# Release Checklist

## Pre-Release
- [x] Update version numbers in `VERSION`, `version.json`, and `Cargo.toml`
- [x] Update `CHANGELOG.md` with release notes
- [x] Run `jankurai security run . --out ops/releases/v1.0.0/security-evidence.json`
- [x] Verify all tests pass
- [x] Generate release artifacts

## Release
- [x] Tag the release with `git tag v1.0.0`
- [x] Push the tag to the remote repository with `git push origin v1.0.0`
- [x] Create a GitHub release

## Post-Release
- [ ] Next-development bump intentionally omitted for `v1.0.0` per release plan
