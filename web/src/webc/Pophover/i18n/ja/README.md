# フローティングバブルプロンプト

マウスがホバーされているとき、または要素がアクティブになっているときに、ホバー ツールチップ情報を表示します。

- ネイティブ Web コンポーネントに基づいており、軽量かつ高パフォーマンス
- 4 方向の適応的な空間位置決めをサポート (class="top/bottom/left/right")
- 黒い曇りガラスのテクスチャーのグラデーション背景と白い文字を使用し、絶妙な物理エッジ輪郭ハイライトを備えています。
- 内蔵の位置適応計算により、ビューポート内の残りのスペースに応じてポップアップの方向が自動的に調整されます。
- `slot="pop"` を使用してフローティング プロンプト コンテンツを表示します

## デモを使用する

```html
<!-- 方向を自動的に計算します (推奨、クラスを宣言する必要はありません) -->
<c-pophover>
  <button>自動</button>
  <div slot="pop">これがバブルチップです</div>
</c-pophover>

<!-- 固定方向 -->
<c-pophover class="left">
  <button>左</button>
  <div slot="pop">これがバブルチップです</div>
</c-pophover>

<c-pophover class="right">
  <button>右</button>
  <div slot="pop">これがバブルチップです</div>
</c-pophover>

<c-pophover class="bottom">
  <button>下</button>
  <div slot="pop">これがバブルチップです</div>
</c-pophover>

<c-pophover class="top">
  <button>優れた</button>
  <div slot="pop">これがバブルチップです</div>
</c-pophover>
```

## CSS変数

カスタム スタイルは、次の CSS 変数を介してサポートされます。

| 変数名 | デフォルト値 | 説明する |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | バブルツールチップとトリガー要素の間のスペース |
| `--pophover-margin` | `24px` | 適応型位置決め計算中のビューポートエッジの安全マージン |
| `--pophover-bg-top` | `#18181ce0` | バブルのグラデーション背景のトップカラー |
| `--pophover-bg-mid` | `#121215e5` | バブルの左右の矢印の背景色 |
| `--pophover-bg-bottom` | `#0c0c0feb` | バブルグラデーション背景の下部と上下の矢印の色 |

## アイコンのツールチップのスタイル (`.Ico`)

純粋なアイコン タイプのフローティング プロンプト ボタン (「編集」、「削除」など) の場合は、 外側のコンテナまたはコンポーネント自体に`.Ico` クラス名を追加することで、統一されたアイコンとホバー スタイルを適用できます。

### 構造例

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">編集</div>
  </c-pophover>
</b>
```

基本的なレイアウト、サイズ（18x18px）、 ホバー色の変更は`.Ico > c-pophover > a`に統一されました。開発者は、次のように、対応するアイコン ファイルを個別に指定するだけで済みます。
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```