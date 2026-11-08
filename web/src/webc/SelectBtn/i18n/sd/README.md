# ڊراپ-ڊائون چونڊ باڪس ۽ بٽڻ جو ميلاپ

- سليڪشن باڪس ۽ بٽڻ کي افقي طور تي ورهايو
- خودڪار طور تي منتخب ٿيل رياست کي مقامي اسٽوريج ۾ محفوظ ڪريو
- مداخلت واري بٽڻ تي ڪلڪ ڪريو ۽ ڪسٽم واقعن کي موڪلڻ

## ڊيمو استعمال ڪريو

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">آپشن 1</option>
    <option value="option2">آپشن 2</option>
  </c-select>
  <button>ڊوڙڻ</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ملڪيت

- `key`: مکيه نالو استعمال ڪيو ويو مقامي اسٽوريج جي استحڪام لاءِ
- `value`: في الحال چونڊيل قدر

## واقعو

- `submit`: موڪليو ويو جڏهن بٽڻ دٻايو ويندو آهي، `e.value` هن وقت چونڊيل قدر کڻندو آهي