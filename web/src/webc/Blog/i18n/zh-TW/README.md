# 部落格文章組件

- **自動年份分組**：依文章日期自動歸類年份，並依年份倒序排列
- **精簡日期格式**：展示時自動剔除年份前綴，僅顯示月與日
- **Markdown 摘要與內文**：內建清單摘要預覽與單篇文章完整正文渲染
- **漸層立體標題**：整合 `H1Txt` 展示大標題與發佈日期

## 使用演示

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["標題", "摘要", "2026-07-27"]
};

// 切換載入指定文章正文（傳空字串切回清單）
blog.load("2026/publish");
</script>
```

## 介面參數

### 屬性與方法

- `blog.prefix`：取得或設定文章連結前綴（如 `"/blog/"`）
- `blog.mdUrl`：取得或設定 Markdown 文章地址產生函數 `(key, lang) => url`
- `blog._`：取得或設定文章列表元資料字典 `{ [key]: [title, summary, date] }`
- `blog.load(key)`：切換載入指定 key 的文章內容，傳空字串 `""` 切回部落格列表