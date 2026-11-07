# HTML və JS kodunun onlayn sazlanması

- Kodun altında sazlama çubuğunu göstərin və CodePen və JSFiddle sazlama portallarını təmin edin
- Seçilmiş sazlama platformasını avtomatik xatırlayın
- HTML, CSS və JS kodunu dinamik şəkildə yaratmaq üçün `.gen` atributu vasitəsilə geri çağırışların yeridilməsini dəstəkləyir
- `language-js` sinif adı ilə uşaq elementləri avtomatik müəyyən edin və JS rejimində işləyin

## Demodan istifadə edin

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Qayıdın [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## İnterfeys parametrləri

### Xüsusiyyətlər və üsullar

- `.gen`: Enjekte edilmiş geri çağırış funksiyası. **Debuq panelinin göstərilməsi üçün bu geri zəng vurulmalıdır. **
  - parametr:
    - `type`: Yerləşdirilən məzmun növü (`HTML = 1`, `JS = 2`).
    - `text`: Uşaq elementdən çıxarılan kod mətni.
  - Qaytarılan dəyər: `[htm, css, js]`