# HTML va JS kodini onlayn tuzatish

- Kod ostida disk raskadrovka panelini ko'rsating va CodePen va JSFiddle disk raskadrovka portallarini taqdim eting
- Tanlangan disk raskadrovka platformasini avtomatik eslab qolish
- HTML, CSS va JS kodlarini dinamik ravishda yaratish uchun `.gen` atributi orqali qayta qo'ng'iroqlarni kiritishni qo'llab-quvvatlaydi
- `language-js` sinf nomiga ega bo'lgan bolalar elementlarini avtomatik aniqlash va JS rejimida ishga tushirish

## Ko‘rsatma ishlatish

```html
<c-htm-demo><pre><code class="language-html">
<h1>Namoyish</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Qaytish [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Interfeys parametrlari

### Xususiyatlari va usullari

- `.gen`: kiritilgan qayta qo'ng'iroq funksiyasi. **Bu qayta qo'ng'iroq disk raskadrovka paneli ko'rsatilishi uchun kiritilishi kerak. **
  - parametr:
    - `type`: kiritilgan kontent turi (`HTML = 1`, `JS = 2`).
    - `text`: To'liq elementdan olingan kod matni.
  - Qaytish qiymati: `[htm, css, js]`