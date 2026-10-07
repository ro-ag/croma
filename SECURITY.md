# Security Policy

## Supported Versions

Security fixes are applied to the latest 1.x minor release only; older 1.x
releases do not receive fixes.

| Version | Supported          |
| ------- | ------------------ |
| 1.3.x   | :white_check_mark: |
| < 1.3   | :x:                |

## Reporting a Vulnerability

Please report security issues privately — do **not** open a public issue. Use
GitHub's **private vulnerability reporting**: the repository's *Security* tab →
**Report a vulnerability**. That opens a private advisory only the maintainers
can see.

You can expect an acknowledgement within 7 days. If the report is accepted, a fix
is prioritized for the next release and you are credited (unless you prefer to
remain anonymous); if declined, we explain why.

Croma is a local, offline conversion toolkit: it parses ABC text and emits
MusicXML (and back). The most relevant risks are crashes, hangs, or excessive
resource use on malformed input — those are valid reports.
