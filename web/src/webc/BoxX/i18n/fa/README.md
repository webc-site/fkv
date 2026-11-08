# یک لایه بازشو با دکمه بستن ایجاد و نمایش دهید

- فراخوانی کاربردی برای ایجاد و نمایش لایه بازشو
- دکمه بسته داخلی، برای بستن و حذف عنصر از DOM کلیک کنید
- برای غیرفعال کردن کادر ورودی یا بستن لایه بازشو، Esc را فشار دهید

## از دمو استفاده کنید

```html
<button>برای پاپ آپ کلیک کنید</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "متن";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## پارامترهای رابط

تابع صادرات پیش‌فرض `HTMLDialogElement` را برمی‌گرداند و کلاس‌های سبک `Box` و `X` به عنصر گفتگو اضافه شده‌اند.

## کلاس سبک

### `.Box.X`

به عنصر `dialog` اعمال می شود، سبک گفتگو را مشخص می کند.

### `a.X`

به عنصر `a` اعمال می شود، سبک دکمه بستن را مشخص می کند.