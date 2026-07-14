# L2TV 3.0.0

L2TVのTauri版を正式リリースします。Electron版の主要な操作感と機能を引き継ぎながら、Tauri 2、WebView2、Rust、SQLiteを採用し、配布容量と実行時負荷を抑えました。

## 主な内容

- LR2 / OpenLR2とbeatorajaに対応
- LR2モードとbeatorajaモードのDBパス、スコア、更新履歴、比較データを完全に分離
- beatoraja固有のMAX、PERFECT、EXHARD、ASSISTなどのクリアランプに対応
- 正規、鏡、乱、R乱、S乱、H乱の使用オプションを表示
- 通常RANDOMとbeatorajaのR-RANDOMについて、保存されたseedからSP 7KEY配置を再現
- beatorajaのプレイ履歴カレンダーをPlayer Data直下に追加
- 難易度表の一覧取得、検索、URL追加、選択、ドラッグ並べ替えに対応
- GENOSIDE2018 SP段位、st/sl段位、歴代Overjoy合格表示に対応
- FORCE RATE、対象51件、前回比、対象譜面の画像出力に対応
- LR2 / OpenLR2モードでローカルRival DBとStellaverse Rival IDによる比較に対応
- Stellaverse IRのプロフィール・順位取得を任意でON/OFF可能
- Electron版から設定や前回解析結果を移すJSON引継ぎ機能を追加
- 旧Electron版を起動できなくても保存領域から引継ぎJSONを作れる`L2TV-Electron-Data-Exporter.exe`を同梱
- 日本語 / English、Light / Darkテーマ、各種PNG出力に対応

## beatorajaモード

beatorajaモードでは`score.db`と`songdata.db`を使用します。Player Data直下のカレンダーは、選択した`score.db`と同じフォルダにある`scoredatalog.db`を読み取ります。beatorajaにはL2TVが参照できるRival DBがないため、ライバル関連UIは表示しません。

## 導入方法

1. 配布された7zを空のフォルダへ解凍します。
2. `L2TV.exe`を起動します。
3. 初回の表示言語を選択します。
4. メニューで`LR2`または`beatoraja`を選び、必要なDBを指定します。
5. 難易度表を選択して`表とランプを読み込む`を押します。

インストーラーは使用しません。削除するときは、解凍して作成したL2TV専用フォルダだけをゴミ箱へ移動してください。

## 動作要件

- Windows 10 / 11 64bit
- Microsoft Edge WebView2 Runtime
- LR2 / OpenLR2: `score.db`と`song.db`
- beatoraja: `score.db`と`songdata.db`
- 難易度表やStellaverse IRの取得にはインターネット接続が必要

## 安全性

- ローカルDBは読み取り専用で開き、外部へアップロードしません。
- Stellaverse IR通信は設定で有効にした場合だけ行います。
- 外部通信にはタイムアウト、同時実行制限、キャッシュ、ローカル/プライベートIP拒否を設定しています。
- CIでRustテスト、フロントエンドテスト、Clippy、Releaseビルド、配布物検証を実施しています。

## 補足

- Electron版とは別アプリです。Electron版からの自動更新は行いません。
- Electron版からの設定移行には、同梱の専用抽出ツール、または両版のメニューにある引継ぎJSON機能を使用してください。
- 専用抽出ツールは旧Electron保存領域を変更せず、一時コピーから読み取ります。作成したJSONにはローカルパスやプレイヤー情報が含まれる場合があるため、公開しないでください。
- L2TV本体はMIT Licenseです。依存ライブラリと外部データの権利表記は`THIRD_PARTY_NOTICES.md`を参照してください。
