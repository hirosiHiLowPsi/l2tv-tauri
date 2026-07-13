# Contributing

## Requirements

- Windows 10 or 11 x64
- Node.js 22
- Rust 1.97.0 with `rustfmt` and `clippy`
- Visual Studio Build Tools 2022, Desktop development with C++
- Microsoft Edge WebView2 Runtime

## Local verification

Run the complete required check before opening a pull request:

```powershell
npm ci
npm run ci
```

To verify the portable archive:

```powershell
npm run package:win
pwsh -NoProfile -File scripts/verify-release.ps1 -VerifyArchive
```

The real WebView smoke test uses local LR2 databases and is intentionally not run in public CI:

```powershell
$env:L2TV_E2E_SCORE_DB = "D:\LR2\LR2files\Database\Score\test-player.db"
$env:L2TV_E2E_SONG_DB = "D:\LR2\LR2files\Database\song.db"
npm run test:e2e
```

Do not commit databases, transfer JSON files, user identifiers, generated `dist` files, `node_modules`, or Rust `target` output.

## Pull requests

- Keep changes scoped and document user-visible behavior.
- Add or update tests for parsing, database, security, and bridge changes.
- Never weaken URL validation, CSP, Tauri capabilities, or read-only database access without a documented security review.
- All CI and security checks must pass before merge.
