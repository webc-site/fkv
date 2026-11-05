# Sayfanın kenarındaki açılır pencereyi sor

- **İşlev çağrısı**: Normal ve hata istemi arayüzleri sağlar
- **Otomatik yığın**: Birden fazla ipucu, aralığı otomatik olarak hesaplar ve dikey olarak yığın
- **Planlanmış ve manuel kapatma**: Planlanmış ve manuel kapatmayı destekler

## Demoyu kullanın

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Bilgi istemi içeriği";
});

toastErr((el) => {
  el.textContent = "İçerik hatası";
});

toast(
  (el) => {
    el.textContent = "99 saniye sonra otomatik olarak kapat";
  },
  99
);
```

## Arayüz parametreleri

### toast(render, timeout)

İstemleri oluşturun ve görüntüleyin.

- `render`: `(el) => void`, geri arama oluşturuluyor, `el` bilgi istemi kutusu öğesidir.
- `timeout`: `Number`, zaman aşımı saniyesi. Varsayılan 9. Otomatik olarak kapanmamak için 0'a ayarlayın.
- Dönüş değeri: bilgi istemi kutusu öğesi. `el.close()` kapalı özelliğini destekler.

### toastErr(render, timeout)

Hata mesajları oluşturun ve görüntüleyin. Parametreler `toast` ile aynıdır ve `.ERR` stil sınıfına sahiptir.

## stil sınıfı

- `.Toast`: İstem kutusu temel sınıfı.
- `.ERR`: Hata durumu.
- `.animated` / `.fadeInR` / `.fadeOutR`: animasyonlu geçiş.
- `.x`: Kapat düğmesi.