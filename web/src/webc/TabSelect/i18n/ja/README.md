# ドロップダウン選択のあるタブページ

- ラベルへの適応幅ドロップダウン選択ボックス (`<c-select>`) の埋め込みをサポートします。これは、現在のタブ ページがアクティブ化されている場合にのみ表示されます。
- 構造仕様: 子要素はラベル ナビゲーションとして `nav > a` を使用し、直接の子要素 ​​`b` はコンテンツ領域として機能します。
- `a[value]` タグ、`a` を `c-select` (グループの値属性と選択したオプション値に基づいて) および `b[slot]` コンテンツ ノードに自動的に関連付けます。
- `key` 属性を介して、現在アクティブ化されている項目を `localStorage` に自動的に永続化することをサポートします。
- `key + '/' + tab` を使用した各グループの履歴サブ選択の `localStorage` への自動保存と復元をサポートします。
- `value` 属性によるアクティブなタブの指定をサポートします。

## デモを使用する

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>言語</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>システム</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript はスクリプト言語です</b>
    <b slot="rs">Rust はシステムレベルのプログラミング言語です</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS は Unix ベースのグラフィカル オペレーティング システムです。</b>
    <b slot="win">Windows は Microsoft によって開発されたオペレーティング システムです</b>
  </b>
</c-tab-select>
```

## 財産

- `key`: localStorage の永続化に使用されるキー名
- `value`: 現在選択されている値

## カスタムイベント

- `change`: タブを切り替えるときにトリガーされます。`e.value` は、現在アクティブ化されているオプションの `value` 値です。