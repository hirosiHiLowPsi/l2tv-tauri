# L2TV Tauri 2 Edition

[![CI](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/ci.yml/badge.svg)](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/ci.yml)
[![Security audit](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/security-audit.yml/badge.svg)](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/security-audit.yml)
[![CodeQL](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/codeql.yml/badge.svg)](https://github.com/hirosiHiLowPsi/l2tv-tauri/actions/workflows/codeql.yml)

L2TVの既存UIと主要機能を、Electronから **Tauri 2 + WebView2 + Rust + SQLite** へ移植したWindows向けデスクトップアプリです。

- 制作: HiLowPsi
- バージョン: 3.1.0
- 対応OS: Windows 10 / 11 64bit
- ライセンス: MIT
- ソース: https://github.com/hirosiHiLowPsi/l2tv-tauri

## 目的

Electron版の操作感を維持しながら、配布容量と実行時メモリを減らすための別プロジェクトです。画面にはElectron版と同じHTML/CSS/JavaScriptを使用し、ローカルDB処理・難易度表取得・Archive推定IR順位をRustで実行します。

## 主な機能

- LR2 / OpenLR2の`score.db`と`song.db`、またはbeatorajaの`score.db`と`songdata.db`を読み取り専用で解析
- LR2 / beatorajaモード切替、モードごとに独立したDBパス・前回解析結果・更新比較、beatoraja固有ランプ（MAX / PERFECT / EXHARD / ASSISTなど）の表示
- 難易度表の検索、URL追加、選択、並べ替え
- ランプ、スコア、BP、プレイ回数、本日更新の表示
- LR2 / OpenLR2 / beatorajaで自己ベストを出した際の配置オプション（正規 / 鏡 / 乱 / R乱 / S乱 / H乱）表示と、通常RANDOM・R-RANDOM・SP 7KEYの保存seedから各ゲーム互換方式で再現した個別鍵盤画像表示（R乱はbeatorajaのみ、ゲージオプションは非表示）
- beatorajaの`scoredatalog.db`を使った年別プレイ履歴カレンダーと日別更新内容を、Player Data直下に常時表示
- GENOSIDE2018 SP段位、st/sl段位、歴代Overjoy合格表示
- FORCE RATE、対象51件、前回比、画像出力
- LR2 / OpenLR2モードでの従来方式のローカルRival DB読み込みによる比較（beatorajaモードではライバル機能を非表示）
- LR2IR Archiveを基にした同梱データによる推定IR順位表示（対象譜面の上位100スコアまで、Stellaverse IRへのオンライン順位取得は使用しません）
- 日本語 / English、Light / Darkテーマ
- 本日更新、難易度表、FORCE RATE対象のPNG保存
- 同梱の専用抽出ツール、またはElectron版のメニューを使ったJSON引継ぎ（設定、難易度表、ライバルID、前回解析結果）

## 動作要件

- Windows 10 / 11 64bit
- Microsoft Edge WebView2 Runtime
- LR2 / OpenLR2を使う場合: `score.db`と`song.db`
- beatorajaを使う場合: プレイヤーフォルダの`score.db`と、beatoraja直下の`songdata.db`
- 難易度表をオンライン取得する場合はインターネット接続

Windows 10 / 11では通常WebView2が導入済みです。起動できない場合はMicrosoft公式のWebView2 Runtimeを導入してください。

## 起動方法

1. 配布された7zを空のフォルダへ解凍します。
2. `L2TV.exe`を起動します。
3. 初回の言語を選択します。
4. メニューで`LR2`または`beatoraja`を選びます。
5. LR2では`score.db`と`song.db`、beatorajaでは`score.db`と`songdata.db`を指定します。
6. 難易度表一覧から使用する表を選び、`表とランプを読み込む`を押します。

beatorajaの現在の自己ベストは`score.db`から取得するため、`scoredatalog.db`の指定は不要です。beatorajaモードではPlayer Data直下にカレンダーが表示され、アプリは選択中の`score.db`と同じフォルダにある`scoredatalog.db`を読み取ります。beatoraja内部のASSIST EASYとLIGHT ASSIST EASYは、L2TV上ではどちらも`ASSIST`として集計します。

カレンダーはbeatorajaモードだけで利用できます。日を選ぶと、その日に保存された曲名、ランプ、スコア、BP、使用オプションと再現可能な鍵盤配置を確認できます。古いbeatorajaの`scoredatalog.db`は譜面ごとの最新履歴だけを保持するため、該当する場合は実際のプレイ数より少なく表示される旨を画面内に表示します。beatorajaにはL2TVが参照できるRival DBがないため、RIVALボタン、ライバル設定、譜面一覧のRival列は表示しません。

LR2とbeatorajaのスコアは統合しません。モードを切り替えると、そのモード専用のDBパスと前回の読み込み結果へ切り替わり、更新差分やFORCE RATEの前回比も同じモード内だけで比較します。

この版はインストーラーを使用しません。削除するときは、L2TV専用の解凍先フォルダだけをゴミ箱へ移動してください。

## Electron版からのデータ引継ぎ

Electron版とTauri版では保存領域が異なるため、専用JSONを介して引き継ぎます。

### 同梱ツールを使う方法

この方法は、旧Electron版が起動できない場合や、引継ぎ機能を搭載する前のElectron版にも使用できます。

1. Electron版L2TVを完全に終了します。
2. 配布フォルダの`L2TV-Electron-Data-Exporter.exe`を起動します。
3. 旧Electron版の解凍先フォルダを選びます。通常は、その中に`lr2ir-table-lamp-viewer-data`があります。`score.db`を選ぶ画面ではありません。
4. 引継ぎJSONの保存先を選び、完了表示を確認します。
5. Tauri版の`L2TV.exe`を起動します。
6. メニューの`その他`から`引継ぎデータを読み込む`を押し、先ほどのJSONを選択します。
7. 上書きを確認すると、画面が再読み込みされて引継ぎが完了します。

抽出ツールは旧保存領域を直接変更せず、一時コピーからデータを読み取ります。外部通信は行わず、処理中だけPC内の`127.0.0.1`を使用します。

### Electron版のメニューを使う方法

引継ぎ対応済みのElectron版を起動できる場合は、Electron版のメニュー`その他`にある`引継ぎデータを書き出す`から同じ形式のJSONを保存できます。その後はTauri版の`引継ぎデータを読み込む`を使用します。

引き継ぐ内容は、ゲームモード、DBパスなどの設定、表示設定、難易度表の選択・順序・追加URL、ローカルRival DB設定、前回の解析結果です。DBファイルそのものはコピーせず、元のファイルを引き続き読み取り専用で参照します。

引継ぎJSONにはローカルファイルのパス、プレイヤー情報、ライバル設定などが含まれる場合があります。公開したり他人へ渡したりせず、移行後に不要なら削除してください。詳しい手順は同梱の`Electron版データ引継ぎツール.txt`にも記載しています。

## データと通信

- `score.db`、`song.db`、`songdata.db`、`scoredatalog.db`、Rival DBは読み取り専用で開き、書き換えません。
- ローカルDBを外部へアップロードしません。
- 難易度表の取得時は、選択した難易度表URLへ通信します。
- IR順位表示は同梱のArchive推定データを使用します。Stellaverse IRへのオンライン順位取得は行いません。
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

通常RANDOMの配置取得も実DBで検証する場合は、`npm run test:e2e`の前に`$env:L2TV_E2E_EXPECT_RANDOM_LAYOUT = "1"`を設定します。beatorajaのR-RANDOMとカレンダーも検証する場合は、さらに`$env:L2TV_E2E_EXPECT_R_RANDOM_LAYOUT = "1"`と`$env:L2TV_E2E_EXPECT_HISTORY = "1"`を設定します。

beatorajaを検証する場合は、上記2パスをbeatorajaのDBへ変え、`$env:L2TV_E2E_GAME_DATA_MODE = "beatoraja"`も設定します。

`test:e2e`に既定のDBパスやプレイヤーIDはありません。公開しても問題のないテストデータを環境変数で指定してください。

## 構成

- `public/`: Electron版と共通のUI、アセット、定数データ
- `src-tauri/src/database.rs`: SQLite読取、段位、FORCE RATE、Rival処理
- `src-tauri/src/openlr2_random.rs`: OpenLR2互換のSP 7KEY通常RANDOM配置生成
- `src-tauri/src/beatoraja_random.rs`: `java.util.Random`互換のbeatoraja SP 7KEY通常RANDOM / R-RANDOM配置生成
- `src-tauri/src/tables.rs`: 難易度表一覧と表データ取得
- `src-tauri/src/stellaverse.rs`: 旧Stellaverse連携処理（3.1.0ではオンライン順位取得に使用しません）
- `src-tauri/src/security.rs`: URLと接続先の検証
- `src-tauri/src/commands.rs`: Tauriネイティブコマンド
- `src-tauri/src/electron_transfer_exporter.rs`: 旧Electron保存領域からの引継ぎJSON抽出

## バージョン3.1.0について

<<<<<<< HEAD
3.1.0では、Stellaverse IRへのオンライン順位取得を使わず、LR2IR Archiveを基にした同梱データで推定IR順位を表示する方式へ変更しました。ライバル機能は従来どおり、LR2 / OpenLR2のローカルRival DBを読み込む方式です。Electron版とは別アプリとして配布し、自動更新は行いません。設定移行には上記の引継ぎJSONを使用してください。L2TVはDBを読み取り専用で扱いますが、大切なゲームデータは通常どおりバックアップを維持してください。

=======
3.1.0では、Stellaverse IRへのオンライン順位取得を使わず、LR2IR Archiveを基にした同梱データで推定IR順位を表示する方式へ変更しました。ライバル機能は従来どおり、LR2 / OpenLR2のローカルRival DBを読み込む方式です。

FORCE RATEでは、未プレイ譜面が対象に入る可能性があった判定を修正しました。特に `Little "Sister" Bitch` と `紅染十三番街道` の同名別難易度譜面を、曲名ではなく譜面MD5で判定するようにし、`Little "Sister" Bitch ★22` / `Little "Sister" Bitch ★10`、`紅染十三番街道 ★22` / `紅染十三番街道 ★19` を別譜面として扱います。

Electron版とは別アプリとして配布し、自動更新は行いません。設定移行には上記の引継ぎJSONを使用してください。L2TVはDBを読み取り専用で扱いますが、大切なゲームデータは通常どおりバックアップを維持してください。

>>>>>>> cb970e4 (Fix FORCE RATE same-title chart identity notes)
詳しい更新内容は同梱のリリースノートを参照してください。

## ライセンス

L2TV本体はMIT Licenseです。詳細は`LICENSE`、依存ライブラリと外部データについては`THIRD_PARTY_NOTICES.md`を参照してください。

## セキュリティ

- npm、Cargo、GitHub ActionsをDependabotで監視します。
- CIでRustテスト、フロントエンドテスト、Clippy、Releaseビルド、配布物検証を実行します。
- JavaScriptはCodeQL、依存パッケージはnpm auditとRustSecで定期検査します。
- 脆弱性を見つけた場合は公開Issueへ詳細を書かず、[Security Policy](SECURITY.md)に従って報告してください。
