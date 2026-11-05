# Blog yazısı bileşeni

- **Otomatik yıl gruplandırma**: Yılları makale tarihine göre otomatik olarak sınıflandırın ve yıllara göre ters sırada düzenleyin
- **Basitleştirilmiş Tarih Formatı**: Görüntülerken yıl önekini otomatik olarak kaldırın ve yalnızca ay ve günü görüntüleyin
- **Markdown özeti ve metin**: Yerleşik liste özeti önizlemesi ve tek makalenin tam metninin oluşturulması
- **Degrade üç boyutlu başlık**: Başlığı ve yayın tarihini görüntülemek için `H1Txt` entegre edin

## Demoyu kullanın

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/yayınla": ["başlık", "özet", "2026-07-27"]
};

// Belirtilen makale gövdesini yüklemek için geçiş yapın (listeye geri dönmek için boş bir dize iletin)
blog.load("2026/publish");
</script>
```

## Arayüz parametreleri

### Özellikler ve yöntemler

- `blog.prefix`: Makale bağlantısı önekini alın veya ayarlayın (`"/blog/"` gibi)
- `blog.mdUrl`: Markdown makale adresi oluşturma işlevini alın veya ayarlayın `(key, lang) => url`
- `blog._`: Makale listesi meta veri sözlüğünü alın veya ayarlayın `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Belirtilen anahtarın makale içeriğini yüklemek için geçiş yapın, blog listesine geri dönmek için boş bir dize `""` iletin