# ڈراپ ڈاؤن سلیکشن باکس اور بٹن کا امتزاج

- سلیکشن باکس اور بٹن کو افقی طور پر الگ کریں۔
- منتخب ریاست کو خودکار طور پر لوکل اسٹوریج میں محفوظ کریں۔
- بٹن کلکس کو روکیں اور حسب ضرورت ایونٹس بھیجیں۔

## ڈیمو استعمال کریں۔

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">آپشن 1</option>
    <option value="option2">آپشن 2</option>
  </c-select>
  <button>چلائیں</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## جائیداد

- `key`: کلیدی نام لوکل سٹوریج استقامت کے لیے استعمال کیا جاتا ہے۔
- `value`: فی الحال منتخب قدر

## واقعہ

- `submit`: بٹن پر کلک کرنے پر بھیج دیا جاتا ہے، `e.value` فی الحال منتخب کردہ قدر رکھتا ہے