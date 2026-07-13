# Security Policy

## Supported versions

Security fixes are applied to the latest published Tauri release or pre-release of L2TV.
Older test builds and the Electron edition are not maintained in this repository.

## Reporting a vulnerability

Do not include vulnerability details, personal data, LR2 databases, or authentication information in a public Issue.

Use GitHub Private Vulnerability Reporting:

https://github.com/hirosiHiLowPsi/l2tv-tauri/security/advisories/new

Please include:

- affected L2TV version
- Windows version
- reproducible steps using non-sensitive sample data
- expected and actual behavior
- potential impact

Never attach a real `score.db`, `song.db`, Rival DB, transfer JSON, or screenshot containing private identifiers unless specifically requested through the private advisory.

## Security boundaries

- LR2/OpenLR2 databases are opened read-only.
- Local databases are not uploaded by L2TV.
- Stellaverse IR communication runs only when the related setting is enabled.
- Remote table and IR requests reject loopback, private, link-local, and otherwise unsafe destinations.
- The WebView is protected by a restrictive Content Security Policy and minimal Tauri capabilities.

## Release integrity

Official release assets are published through GitHub Releases with SHA-256 checksums. Release builds are produced by GitHub Actions and receive build provenance attestations when published from a version tag.
