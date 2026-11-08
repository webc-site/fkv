# 非同步加載並展示卡片列表

- **非同步載入**：透過非同步函數取得數據
- **狀態展示**：支援載入中、空資料、渲染卡片三種狀態
- **彈性佈局**：卡片折行排列

## 使用演示

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["標題 " + i, "描述 " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## 介面參數

### CardLi(loader)

- **loader**：`() => Promise<Array<[string, string, string]>>`，非同步資料載入函數。每個子項格式為 `[名稱, 簡介, 連結]`。

## 樣式類

- `.CardLi`：外層元素
- `.Card`：卡片元素
- `.Ing`：載入中動畫元素