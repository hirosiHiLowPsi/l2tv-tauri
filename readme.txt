L2TV Tauri 2 Edition 3.1.0
制作: HiLowPsi

【起動】
1. 7zを空のフォルダへ解凍します。
2. L2TV.exeを起動します。
3. メニューでLR2またはbeatorajaを選択します。
4. LR2ではscore.dbとsong.db、beatorajaではscore.dbとsongdata.dbを指定します。
5. 難易度表を選択して「表とランプを読み込む」を押します。

beatorajaの現在の自己ベスト表示にはscoredatalog.dbの指定は不要です。
beatorajaモードではPlayer Data直下に「プレイ履歴カレンダー」を表示します。score.dbと同じフォルダのscoredatalog.dbを読み取り、年別の活動量と日別の曲・ランプ・スコア・BP・使用オプションを確認できます。
旧形式のscoredatalog.dbでは譜面ごとの最新履歴のみ表示される場合があります。
ASSIST EASYとLIGHT ASSIST EASYは、どちらもASSISTとして集計します。
beatorajaモードではRIVALボタン、ライバル設定、譜面一覧のRival列を表示しません。LR2モードでは従来どおり、ローカルRival DBを読み込んで比較します。

R-RANDOMはbeatorajaモードで「R乱」と表示し、保存されたseedから公式実装互換のSP 7KEY配置を再現します。
LR2とbeatorajaのDBパス、スコア、更新差分はモードごとに分けて保存・表示します。

【Electron版からの引継ぎ】
1. Electron版L2TVを完全に終了します。
2. 同梱のL2TV-Electron-Data-Exporter.exeを起動します。
3. 旧Electron版の解凍先フォルダを選びます。score.dbを選ぶ画面ではありません。
4. 引継ぎJSONを保存します。
5. Tauri版L2TVのメニュー「その他」で「引継ぎデータを読み込む」を押します。
6. JSONを選び、上書きを確認すると設定、難易度表、ローカルRival DB設定、前回解析結果が移ります。

旧Electron版を起動できる場合は、Electron版のメニューからJSONを書き出す従来の方法も使用できます。
抽出ツールは旧保存データを変更せず、一時コピーから読み取ります。外部サーバーには送信しません。
引継ぎJSONにはローカルパスやプレイヤー情報が含まれる場合があるため、公開しないでください。
詳しくはElectron版データ引継ぎツール.txtを参照してください。

【必要環境】
- Windows 10 / 11 64bit
- Microsoft Edge WebView2 Runtime
- LR2 / OpenLR2のscore.dbとsong.db、またはbeatorajaのscore.dbとsongdata.db

【安全面】
- score.db、song.db、songdata.db、scoredatalog.dbは読み取り専用で扱い、書き換えません。
- ローカルDBを外部へアップロードしません。
- IR順位表示は同梱のArchive推定データを使用します。Stellaverse IRへのオンライン順位取得は行いません。
- FORCE RATE対象判定を修正し、未プレイ譜面が対象に入る可能性を減らしました。
- 同名別譜面の判定を修正し、Little "Sister" Bitch ★22 / ★10、紅染十三番街道 ★22 / ★19 を別譜面として扱います。

この版はインストーラーを使いません。削除するときは、解凍して作成したL2TV専用フォルダだけをゴミ箱へ移動してください。

詳細はREADME.mdを参照してください。
