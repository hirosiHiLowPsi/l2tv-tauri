# L2TV Tauri 2 Edition

[![CI](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/ci.yml/badge.svg)](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/ci.yml)
[![Security audit](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/security-audit.yml/badge.svg)](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/security-audit.yml)
[![CodeQL](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/codeql.yml/badge.svg)](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/codeql.yml)

L2TVの既存UIと主要機能を、Electronから **Tauri 2 + WebView2 + Rust + SQLite** へ移植したWindows向けプレテスト版です。

- 制作: HiLowPsi
- バージョン: 2.1.0 pretest
- 対応OS: Windows 10 / 11 64bit
- ライセンス: MIT
- ソース: https://github.com/hirosiHiLowPsi/l2tv-tauri

## 目的

Electron版の操作感を維持しながら、配布容量と実行時メモリを減らすための別プロジェクトです。画面にはElectron版と同じHTML/CSS/JavaScriptを使用し、ローカルDB処理・難易度表取得・Stellaverse IR連携をRustで実行します。

## 主な機能

- LR2 / OpenLR2の`score.db`と`song.db`を読み取り専用で解析
- 難易度表の検索、URL追加、選択、並べ替え
- ランプ、スコア、BP、プレイ回数、本日更新の表示
- GENOSIDE2018 SP段位、st/sl段位、歴代Overjoy合格表示
- FORCE RATE、対象51件、前回比、画像出力
- ローカルRival DBとStellaverse Rival IDによる比較
- Stellaverse IRプロフィール・順位取得のON/OFF
- 日本語 / English、Light / Darkテーマ
- 本日更新、難易度表、FORCE RATE対象のPNG保存
- Electron版とのJSON引継ぎ（設定、難易度表、ライバルID、前回解析結果）

## 動作要件

- Windows 10 / 11 64bit
- Microsoft Edge WebView2 Runtime
- LR2系の`score.db`
- LR2系の`song.db`
- 難易度表やStellaverse IRを使用する場合はインターネット接続

Windows 10 / 11では通常WebView2が導入済みです。起動できない場合はMicrosoft公式のWebView2 Runtimeを導入してください。

## 起動方法

1. 配布された7zを空のフォルダへ解凍します。
2. `L2TV.exe`を起動します。
3. 初回の言語を選択します。
4. メニューから`score.db`と`song.db`を指定します。
5. 難易度表一覧から使用する表を選び、`表とランプを読み込む`を押します。

この版はインストーラーを使用しません。削除するときは、L2TV専用の解凍先フォルダだけをゴミ箱へ移動してください。

## Electron版からのデータ引継ぎ

Electron版とTauri版では保存領域が異なるため、専用JSONを介して引き継ぎます。

1. 引継ぎ対応済みのElectron版を、従来の`lr2ir-table-lamp-viewer-data`フォルダが隣にある状態で起動します。
2. メニューの`その他`から`引継ぎデータを書き出す`を押し、JSONを保存します。
3. Tauri版のL2TVを起動します。
4. メニューの`その他`から`引継ぎデータを読み込む`を押し、先ほどのJSONを選択します。
5. 上書きを確認すると、画面が再読み込みされて引継ぎが完了します。

引き継ぐ内容は、DBパスなどの設定、表示設定、難易度表の選択・順序・追加URL、Stellaverse Rival ID、前回の解析結果です。`score.db`や`song.db`そのものはコピーせず、元のファイルを引き続き読み取り専用で参照します。

## データと通信

- `score.db`、`song.db`、Rival DBは読み取り専用で開き、書き換えません。
- ローカルDBを外部へアップロードしません。
- 難易度表の取得時は、選択した難易度表URLへ通信します。
- Stellaverse IR連携はメニューで有効にした場合だけ通信します。
- 外部通信にはタイムアウト、同時実行制限、キャッシュ、ローカル/プライベートIP拒否を設定しています。
- アプリ設定はWindowsユーザーごとのWebView2領域に保存されます。

## 開発

必要なもの:

- Node.js 22以降
- Rust stable
- Visual Studio Build Tools 2022の`Desktop development with C++`
- WebView2 Runtime

```powershell
npm install
npm test
npm run dev
npm run build
```

実WebViewとローカルDBを使うスモークテスト:

```powershell
$env:L2TV_E2E_SCORE_DB = "D:\LR2\LR2files\Database\Score\test-player.db"
$env:L2TV_E2E_SONG_DB = "D:\LR2\LR2files\Database\song.db"
npm run test:e2e
```

`test:e2e`に既定のDBパスやプレイヤーIDはありません。公開しても問題のないテストデータを環境変数で指定してください。

## 構成

- `public/`: Electron版と共通のUI、アセット、定数データ
- `src-tauri/src/database.rs`: SQLite読取、段位、FORCE RATE、Rival処理
- `src-tauri/src/tables.rs`: 難易度表一覧と表データ取得
- `src-tauri/src/stellaverse.rs`: Stellaverse IR連携
- `src-tauri/src/security.rs`: URLと接続先の検証
- `src-tauri/src/commands.rs`: Tauriネイティブコマンド

## プレテストについて

この成果物はTauri移植版の動作確認用です。Electron版からの自動更新は行いません。設定移行には上記の引継ぎJSONを使用してください。大切なDBは通常どおりバックアップを維持してください。

## ライセンス

L2TV本体はMIT Licenseです。詳細は`LICENSE`、依存ライブラリと外部データについては`THIRD_PARTY_NOTICES.md`を参照してください。

## セキュリティ

- npm、Cargo、GitHub ActionsをDependabotで監視します。
- CIでRustテスト、フロントエンドテスト、Clippy、Releaseビルド、配布物検証を実行します。
- JavaScriptはCodeQL、依存パッケージはnpm auditとRustSecで定期検査します。
- 脆弱性を見つけた場合は公開Issueへ詳細を書かず、[Security Policy](SECURITY.md)に従って報告してください。
