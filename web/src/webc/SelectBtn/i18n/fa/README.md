# ترکیبی از جعبه انتخاب کشویی و دکمه

- کادر انتخاب و دکمه را به صورت افقی به هم متصل کنید
- حالت انتخاب شده را به صورت خودکار در localStorage ذخیره کنید
- رهگیری کلیک های دکمه و ارسال رویدادهای سفارشی

## از دمو استفاده کنید

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">گزینه 1</option>
    <option value="option2">گزینه 2</option>
  </c-select>
  <button>اجرا کنید</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## دارایی

- `key`: نام کلید مورد استفاده برای ماندگاری localStorage
- `value`: مقدار در حال حاضر انتخاب شده است

## رویداد

- `submit`: هنگامی که روی دکمه کلیک می‌شود ارسال می‌شود، `e.value` مقدار انتخابی فعلی را حمل می‌کند