# اشکال زدایی کدهای HTML و JS به صورت آنلاین

- یک نوار اشکال زدایی در زیر کد ارائه کنید و پورتال اشکال زدایی CodePen و JSFiddle را ارائه دهید
- به طور خودکار پلت فرم اشکال زدایی انتخاب شده را به خاطر بسپارید
- پشتیبانی از تزریق تماس از طریق ویژگی `.gen` برای تولید پویا کد HTML، CSS و JS
- عناصر فرزند را با `language-js` نام کلاس شناسایی کنید و در حالت JS اجرا کنید

## از دمو استفاده کنید

```html
<c-htm-demo><pre><code class="language-html">
<h1>نسخه ی نمایشی</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // بازگشت [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## پارامترهای رابط

### خواص و روش ها

- `.gen`: تابع پاسخ به تماس تزریقی. **این فراخوان باید برای نمایش نوار اشکال زدایی تزریق شود. **
  - پارامتر:
    - `type`: نوع محتوای تزریقی (`HTML = 1`، `JS = 2`).
    - `text`: متن کد استخراج شده از عنصر فرزند.
  - مقدار بازگشتی: `[htm, css, js]`