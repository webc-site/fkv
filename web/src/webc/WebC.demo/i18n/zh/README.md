# 渲染并展示组件的演示、说明文档与安装说明

- 提取并解析 README 说明文档的标题、简介与内容
- 沙盒运行组件交互演示并隔离样式
- 转换文档中的代码块为交互式演示
- 自适应布局，宽屏分栏展示，窄屏标签页切换展示

## 使用演示

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

// 初始化并获取操作接口
const [setMod, setMd, unMount] = WebCDemo(el);
```

## 接口参数

### `WebCDemo(el)`

初始化组件。

- **参数**
  - `el`：`HTMLElement`，组件容器。
- **返回值**：`[setMod, setMd, unMount]`
  - `setMod(mod)`：设置演示模块。
  - `setMd(name, md, lang)`：设置组件名称、README 文档内容及当前语言。
  - `unMount()`：卸载组件并清理事件监听器。

### 演示模块 `mod` 属性

- `CSS`：`string`，组件的样式表路径。
- `HTM`：`string`，组件的 HTML 模板。
- `default`：`function(root)`，组件初始化函数，接收 shadow root 容器。

## 样式类

### `.M`

内容区域。

### `.L`

左侧面板，包含使用说明与文档。

### `.R`

右侧面板，包含演示。

### `c-nav`

窄屏导航栏。