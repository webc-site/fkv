# ایجاد و نمایش لایه بازشو

- فراخوانی کاربردی برای ایجاد و نمایش لایه بازشو
- هنگامی که بسته است، عناصر را به طور خودکار از DOM حذف کنید
- از رفتار لغو کلید Esc جلوگیری کنید

## از دمو استفاده کنید

```html
<button>برای پاپ آپ کلیک کنید</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "عنوان";
  description.textContent = "متن";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## پارامترهای رابط

### توابع پیش فرض صادر شده

`Box()`

- **مقدار بازگشتی**: `HTMLDialogElement`، عنصر گفتگوی کلاس سبک `Box` اضافه شده است.

## کلاس سبک

### `.Box`

روی عنصر `dialog` برای تنظیم موقعیت‌یابی، وسط و تاری ماسک پس‌زمینه اعمال می‌شود.

### `.Lg`

یک کلاس سبک برای محتوای داخلی اعمال می‌شود، رنگ پس‌زمینه، گوشه‌های گرد، طرح‌بندی سایه و عمودی را تنظیم می‌کند و طرح‌بندی داخلی `h3` و `p` را تعریف می‌کند.