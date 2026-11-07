# 异步加载并展示卡片列表

- **异步加载**：通过异步函数获取数据
- **状态展示**：支持加载中、空数据、渲染卡片三种状态
- **弹性布局**：卡片折行排列

## 使用演示

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["标题 " + i, "描述 " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## 接口参数

### CardLi(loader)

- **loader**：`() => Promise<Array<[string, string, string]>>`，异步数据加载函数。每个子项格式为 `[名称, 简介, 链接]`。

## 样式类

- `.CardLi`：外层元素
- `.Card`：卡片元素
- `.Ing`：加载中动画元素