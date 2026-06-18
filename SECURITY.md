# Security Policy

## Supported Versions

Croma is pre-1.0. Security fixes are applied to the latest released version only.

| Version | Supported          |
| ------- | ------------------ |
| 0.9.x   | :white_check_mark: |
| < 0.9   | :x:                |

## Reporting a Vulnerability

Please report security issues privately — do **not** open a public issue.

- Use GitHub's **private vulnerability reporting** ("Report a vulnerability" under
  the repository's *Security* tab), or
- email the maintainer at <removed> with details and reproduction steps.

You can expect an acknowledgement within 7 days. If the report is accepted, a fix
is prioritized for the next release and you are credited (unless you prefer to
remain anonymous); if declined, we explain why.

Croma is a local, offline conversion toolkit: it parses ABC text and emits
MusicXML (and back). The most relevant risks are crashes, hangs, or excessive
resource use on malformed input — those are valid reports.
