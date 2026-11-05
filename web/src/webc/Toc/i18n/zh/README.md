# 目录导航组件

- **响应式抽屉**：宽屏展示侧边栏，窄屏自动收起并配合 `c-side` 抽屉与标题触发按钮
- **多语言驱动**：支持对象字典或多语言函数，自动响应语言切换
- **自动子目录**：自动解析目标容器内的 `h1`~`h6` 标题生成二级目录并监听 Hash 视口滚动高亮
- **异步加载**：点击目录项自动异步获取 Markdown 文本并注入内容区域，显示 `Ing` 加载动画

## 使用演示

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "使用组件"
});
</script>
```

## 接口参数

### 属性与方法

- `toc.prefix`：获取或设置路由前缀字符串（例如 `"doc"`，结尾斜杠可选），`README` 键会自动映射为空路径（即前缀本身）
- `toc.li`：获取或设置目录多语言生成函数 `(lang) => ({ key: title })`
- `toc.mdUrl`：获取或设置 Markdown URL 生成函数 `(key, lang) => url`
- `toc.target`：获取或设置关联的目标渲染节点（如 `c-md`），未指定时自动寻找 `c-md` / `main` / `article`
- `toc.load(key)`：手动触发对应 key 的文档加载与目录展开
- `toc.sync()`：手动重新解析当前目标节点中的标题，同步二级目录与高亮状态
