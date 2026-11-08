# Ochiladigan tanlash oynasi va tugma birikmasi

- Tanlash qutisi va tugmani gorizontal ravishda bir-biriga ulang
- Tanlangan holatni avtomatik ravishda localStorage-ga saqlang
- Tugma bosishlarini to'xtating va maxsus voqealarni jo'nating

## Ko‘rsatma ishlatish

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Variant 1</option>
    <option value="option2">Variant 2</option>
  </c-select>
  <button>yugur</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## mulk

- `key`: localStorage barqarorligi uchun ishlatiladigan kalit nomi
- `value`: hozirda tanlangan qiymat

## voqea

- `submit`: tugma bosilganda yuboriladi, `e.value` joriy tanlangan qiymatni o'z ichiga oladi