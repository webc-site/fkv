# Dizin Gezinme Bileşeni

- **Duyarlı çekmece**: Kenar çubuğunu geniş ekranda görüntüleyin, dar ekranda otomatik olarak daraltın ve `c-side` çekmecesi ve başlık tetikleyici düğmesiyle birlikte çalışın
- **Çoklu dil sürücüsü**: Nesne sözlüğünü veya çoklu dil işlevlerini destekler, dil değiştirmeye otomatik olarak yanıt verir
- **Otomatik alt dizin**: İkincil bir dizin oluşturmak ve Karma görünüm alanı kaydırma vurgusunu izlemek için hedef kapsayıcıdaki `h1`~`h6` başlıklarını otomatik olarak ayrıştırın.
- **Eşzamansız yükleme**: Markdown metnini otomatik olarak eşzamansız olarak almak ve içerik alanına eklemek için bir dizin öğesine tıklayın ve `Ing` yükleme animasyonunu görüntüleyin

## Demoyu kullanın

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "kullan": "bileşeni kullan"
});
</script>
```

## Arayüz parametreleri

### Özellikler ve yöntemler

- `toc.prefix`: Yönlendirme öneki dizesini alın veya ayarlayın (örneğin, `"doc"`, sondaki eğik çizgi isteğe bağlıdır), `README` anahtarı otomatik olarak boş bir yola (yani önekin kendisine) eşlenecektir.
- `toc.li`: Dizin çoklu dil oluşturma işlevini alın veya ayarlayın `(lang) => ({ key: title })`
- `toc.mdUrl`: Markdown URL oluşturma işlevini edinin veya ayarlayın `(key, lang) => url`
- `toc.target`: İlişkili hedef oluşturma düğümünü alın veya ayarlayın (`c-md` gibi). Belirtilmemişse otomatik olarak `c-md` / `main` / `article`'i bulacaktır.
- `toc.load(key)`: Anahtara karşılık gelen belge yüklemeyi ve dizin genişletmeyi manuel olarak tetikleyin
- `toc.sync()`: Geçerli hedef düğümdeki başlığı manuel olarak yeniden ayrıştırın, ikincil dizini senkronize edin ve durumu vurgulayın