# 切换界面语言的按钮与弹窗

- 点击按钮弹出语言选择浮层。
- 自动高亮当前选中的语言。

## 使用演示

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// 用下面的 JS 代码监听语言变更
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## 接口参数

### 自定义标签 `<c-i18n>`

自定义 HTML 标签，引入 JS 后在 HTML 中使用。

## 状态管理

使用 `src/js/i18n.js` 管理全局语言状态：

- **`LANG_LI`**：语言列表数组，格式为 `[[名称, ID], ...]`。
- **`lang()`**：获取当前语言 ID。
- **`langCode()`**：获取当前语言编码。
- **`langSet(id)`**：设置语言 ID 并通知订阅者。
- **`onLang(func)`**：订阅语言变更，若已设置语言则立即触发回调。返回取消订阅函数。

## 样式类

### `.BtnC.lang`

图标按钮。

### `.I18n.Lg`

弹窗主体布局，用于包裹语言选项。