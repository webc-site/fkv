# HTML ve JS kodunda çevrimiçi hata ayıklama

- Kodun altında bir hata ayıklama çubuğu oluşturun ve CodePen ve JSFiddle hata ayıklama portalları sağlayın
- Seçilen hata ayıklama platformunu otomatik olarak hatırla
- HTML, CSS ve JS kodunu dinamik olarak oluşturmak için `.gen` özelliği aracılığıyla geri aramaların eklenmesini destekler
- `language-js` sınıf adına sahip alt öğeleri otomatik olarak tanımlayın ve JS modunda çalıştırın

## Demoyu kullanın

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // [html, css, js]'yi döndür
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Arayüz parametreleri

### Özellikler ve yöntemler

- `.gen`: Geri çağırma işlevi enjekte edildi. **Hata ayıklama çubuğunun görüntülenmesi için bu geri aramanın eklenmesi gerekir. **
  - parametre:
    - `type`: Eklenen içerik türü (`HTML = 1`, `JS = 2`).
    - `text`: Alt öğeden çıkarılan kod metni.
  - Dönüş değeri: `[htm, css, js]`