# カードリストを非同期でロードして表示する

- **非同期読み込み**: 非同期関数を通じてデータを取得します。
- **ステータス表示**: ロード中、空のデータ、レンダリング カードの 3 つの状態をサポートします。
- **柔軟なレイアウト**: カードは列に配置されます

## デモを使用する

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["タイトル " + i, "説明 " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## インターフェースパラメータ

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`、非同期データ読み込み関数。各サブキーの形式は`[名前、紹介、リンク]` です。

## スタイルクラス

- `.CardLi`: 外部要素
- `.Card`: カード要素
- `.Ing`: アニメーション要素を読み込み中