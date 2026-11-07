# Bileşen demolarını, belgelerini ve kurulum talimatlarını oluşturun ve görüntüleyin

- README belgesinin başlığını, girişini ve içeriğini çıkarın ve ayrıştırın
- Bileşenlerin etkileşimli demolarını çalıştırmak ve stilleri izole etmek için korumalı alan
- Belgelerdeki kod bloklarını etkileşimli sunumlara dönüştürün
- Uyarlanabilir düzen, geniş ekran sütunlu ekran, dar ekran sekmesi değiştirme ekranı

## Demoyu kullanın

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//İşlem arayüzünü başlatıp elde ediyoruz
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Arayüz parametreleri

### `WebCDemo(el)`

Bileşeni başlatın.

- **parametre**
  - `el`:`HTMLElement`, bileşen kapsayıcısı.
- **Dönüş değeri**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Demo modülünü kurun.
  - `setMd(name, md, lang)`: Bileşen adını, README belgesi içeriğini ve geçerli dili ayarlayın.
  - `unMount()`: Bileşeni kaldırın ve olay dinleyicisini temizleyin.

### Demo modülü `mod` özelliği

- `CSS`:`string`, bileşenin stil sayfası yolu.
- `HTM`:`string`, bileşenin HTML şablonu.
- `default`:`function(root)`, bileşen başlatma işlevi, gölge kök kapsayıcısını alır.

## stil sınıfı

### `.M`

içerik alanı.

### `.L`

Sol panelde talimatlar ve belgeler bulunur.

### `.R`

Demoyu içeren sağ panel.

### `c-nav`

Dar ekran gezinme çubuğu.