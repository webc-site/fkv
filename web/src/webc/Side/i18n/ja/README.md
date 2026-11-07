# 引き出しパネルは横からスライドして取り出せます

- 内部スクロール構造を自動的に作成する
- 外側の領域をクリックすると、自動的に折りたたまれます
- 左右にスワイプして閉じるジェスチャ
- 左右方向のサイドスライド位置
- オープンイベントとクローズイベントを提供する

## デモを使用する

```html
<button class="open-btn">開ける</button>

<c-side class="right">
  <div>引き出しの中身</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## インタフェース

### 方法

- `open(el)`: エクスポート機能、ドロワーを開く
- `close(el)`: エクスポート機能、ドロワーを閉じる
- `el.open()`: インスタンス メソッド、ドロワーを開く
- `el.close()`: インスタンス メソッド、ドロワーを閉じます

### イベント

- `open`: オンにすると起動します
- `close`: 閉じたときに起動します

## スタイルクラス

- `.right`: 右スライドアウトの位置
- `.top`: スライドアウトの位置
- `.bottom`: スライドして位置から外します
- (設定されていない場合、デフォルトでは左にスライドアウトします)
- `.open`: オープンステータス
- `.H`: 非表示ステータス
- `.T`: アニメーションの遷移状態