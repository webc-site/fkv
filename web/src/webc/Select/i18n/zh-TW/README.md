# 下拉選擇，寬度自適應目前項

原生 `select` 寬度預設由最長選項文字決定，切換短選項時留白過多，影響美觀。

本元件將原生 `select` 透明覆蓋在 `b` 標籤上方，實現寬度隨目前選取項文字長度精準自適應。

- 保持原生 `select` 在各端的下拉式選單樣式及無障礙存取支援
- 支援完整的鍵盤 Tab 導航與方向鍵切換等原生互動操作
- 寬度自適應目前選項，消除多餘留白

## 使用演示

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```