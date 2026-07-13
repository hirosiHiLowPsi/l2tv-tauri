# Third-Party Notices

L2TV Tauri 2 Edition uses third-party software and public BMS-related data sources.

## Runtime and libraries

- Tauri 2: Apache-2.0 / MIT
- Microsoft Edge WebView2 Runtime: Microsoft license terms
- Rust standard library and Cargo dependencies: their respective licenses
- SQLite (bundled through `rusqlite`): public domain
- Reqwest, Tokio, Serde, Scraper and supporting Rust crates: their respective licenses

Exact Rust package versions are recorded in `src-tauri/Cargo.lock`. Build tooling versions are recorded in `package-lock.json`.

## External data and services

L2TV can read or refer to:

- Lunatic Rave 2 / OpenLR2 local database files
- Public BMS difficulty tables selected by the user
- LR2IR Archive statistics used to prepare FORCE RATE constants
- Stellaverse IR public player and score information when enabled by the user

Rights to BMS charts, songs, difficulty tables, IR services and related data belong to their respective creators and operators. L2TV is not an official application of Lunatic Rave 2, OpenLR2, LR2IR Archive, Stellaverse IR or any difficulty-table operator.
