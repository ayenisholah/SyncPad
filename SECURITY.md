# Security Policy

## Supported versions

SyncPad is pre-1.0; only the latest commit on `main` receives security fixes.

| Version | Supported |
|---|---|
| `main` | Yes |
| Anything else | No |

## Reporting a vulnerability

Please do **not** open a public issue for security vulnerabilities.

Report privately via GitHub's
[private vulnerability reporting](https://github.com/ayenisholah/SyncPad/security/advisories/new)
or by email to <ayenisholah@yahoo.com> with a description of the issue, steps
to reproduce, and the potential impact.

You can expect an acknowledgment within 72 hours. Please allow a reasonable
window for a fix before any public disclosure.

## Scope notes

SyncPad has no accounts by design: an unguessable document slug is the only
access capability. Reports about slug enumeration or prediction, resource
exhaustion (operation floods, oversized documents or messages, connection
churn), snapshot file path handling, and WebSocket message parsing are
especially valuable. Reports that anyone with a document's link can edit it
describe intended behavior, not a vulnerability.
