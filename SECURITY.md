# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| latest release (`1.x` once published) | ✅ |
| pre-releases / `0.x` | best effort |

## Reporting a vulnerability or a destructive bug

Please report privately — **do not open a public issue** for:

- any way to make NANOFreeDisk delete, move or modify a file outside what the user explicitly selected (SafetyGate bypass: junction/symlink tricks, path normalization, race conditions, rule-pack injection);
- privilege escalation through the elevated helper or the installer;
- supply-chain issues in dependencies or the release pipeline;
- the updater accepting unsigned or tampered packages.

Use GitHub's private vulnerability reporting: <https://github.com/congluuit/NANOFreeDisk/security/advisories/new>
or email the author via the contact on <https://nanodev.io.vn/>.

Please include: version, Windows build, exact steps or a proof-of-concept, and what was (or could be) affected. You will get an acknowledgement within 72 hours and a fix or mitigation plan within 14 days for confirmed SafetyGate issues.

## Verifying releases

Every release ships `SHA256SUMS.txt` and a minisign signature `SHA256SUMS.txt.minisig`. The public key is published in the release notes and in this file once the first signed release is out.

```text
minisign public key: (to be published with the first signed release)
```
