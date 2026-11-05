# 博客文章组件

- **自动年份分组**：根据文章日期自动归类年份，并按年份倒序排列
- **精简日期格式**：展示时自动剔除年份前缀，仅显示月与日
- **Markdown 摘要与正文**：内置列表摘要预览与单篇文章完整正文渲染
- **渐变立体标题**：集成 `H1Txt` 展示大标题与发布日期

## 使用演示

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["标题", "摘要", "2026-07-27"]
};

// 切换加载指定文章正文（传空字符串切回列表）
blog.load("2026/publish");
</script>
```

## 接口参数

### 属性与方法

- `blog.prefix`：获取或设置文章链接前缀（如 `"/blog/"`）
- `blog.mdUrl`：获取或设置 Markdown 文章地址生成函数 `(key, lang) => url`
- `blog._`：获取或设置文章列表元数据字典 `{ [key]: [title, summary, date] }`
- `blog.load(key)`：切换加载指定 key 的文章内容，传空字符串 `""` 切回博客列表
