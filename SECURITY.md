# Security policy

Pusmogo runs fully on your Mac. It needs Microphone and Accessibility permissions, so we take security reports seriously.

## Supported versions

Pusmogo has not had a stable release yet. Security fixes go to the `main` branch and to the latest release once releases exist.

## Reporting a vulnerability

Please **do not open a public issue** for a security problem.

Report it privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability** (private vulnerability reporting). Include:

- what the problem is and what an attacker could do with it,
- steps to reproduce, and the macOS and Pusmogo versions,
- any suggested fix.

We aim to acknowledge reports within 5 working days and to agree on a disclosure date with you. We will credit you in the release notes unless you prefer otherwise.

## Scope

In scope: the Pusmogo app, the Rust core, the MCP server (`pusmogo-mcp`), the build scripts and the model download.

Out of scope: vulnerabilities in third-party speech models or in macOS itself (report those upstream), and issues that need an already compromised Mac.
