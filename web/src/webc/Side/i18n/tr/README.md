# Yandan dışarı kayan çekmece panelleri

- Dahili kaydırma yapılarını otomatik olarak oluşturun
- Otomatik olarak daraltmak için dış alanı tıklayın
- Kapatmak için sola ve sağa kaydırın
- Sol ve sağ yönlerde yan kayar konumlandırma
- Açık ve kapalı etkinlikler sağlayın

## Demoyu kullanın

```html
<button class="open-btn">Açık</button>

<c-side class="right">
  <div>Çekmece içeriği</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## arayüz

### yöntem

- `open(el)`: Dışa aktarma işlevi, çekmeceyi açma
- `close(el)`: Dışa aktarma işlevi, çekmeceyi kapatma
- `el.open()`: örnek yöntemi, çekmeceyi aç
- `el.close()`: örnek yöntemi, çekmeceyi kapatın

### etkinlik

- `open`: Açıldığında etkinleşir
- `close`: Kapatıldığında etkinleşir

## stil sınıfı

- `.right`: sağa kaydırmalı konumlandırma
- `.top`: Konumlandırmayı dışarı kaydırın
- `.bottom`: konumlandırmanın dışına kaydırın
- (Ayarlanmadıysa varsayılan, sola doğru kaydırmaktır)
- `.open`: açık durum
- `.H`: gizli durum
- `.T`: animasyon geçiş durumu