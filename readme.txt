L2TV Tauri 2 Edition 2.1.0 pretest
制作: HiLowPsi

【起動】
1. 7zを空のフォルダへ解凍します。
2. L2TV.exeを起動します。
3. メニューからscore.dbとsong.dbを指定します。
4. 難易度表を選択して「表とランプを読み込む」を押します。

【Electron版からの引継ぎ】
1. 引継ぎ対応済みElectron版のメニュー「その他」で「引継ぎデータを書き出す」を押します。
2. Tauri版の同じ画面で「引継ぎデータを読み込む」を押します。
3. JSONを選び、上書きを確認すると設定、難易度表、ライバルID、前回解析結果が移ります。

Electron版は、従来のlr2ir-table-lamp-viewer-dataフォルダが隣にある場所から起動してください。

【必要環境】
- Windows 10 / 11 64bit
- Microsoft Edge WebView2 Runtime
- LR2 / OpenLR2のscore.dbとsong.db

【安全面】
- DBは読み取り専用で扱い、書き換えません。
- ローカルDBを外部へアップロードしません。
- Stellaverse IR通信は、メニューで有効にした場合だけ行います。

この版はインストーラーを使いません。削除するときは、解凍して作成したL2TV専用フォルダだけをゴミ箱へ移動してください。

詳細はREADME.mdを参照してください。
