# yüzen kabarcık istemi

Fare üzerine gelindiğinde veya öğe etkinleştirildiğinde vurgulu araç ipucu bilgilerini görüntüleyin.

- Yerel Web Bileşenini temel alan, hafif ve yüksek performanslı
- Dört yönde uyarlanabilir mekansal konumlandırmayı destekler (sınıf = "üst/alt/sol/sağ")
- Zarif fiziksel kenar kontur vurgularına sahip, siyah buzlu cam dokulu degrade arka plan ve beyaz karakterler kullanır.
- Yerleşik konum uyarlamalı hesaplama, açılır pencere yönünü görünüm alanında kalan alana göre otomatik olarak ayarlar
- Değişken bilgi istemi içeriğini görüntülemek için `slot="pop"` kullanın

## Demoyu kullanın

```html
<!-- Yönü otomatik olarak hesapla (önerilir, sınıf bildirmeye gerek yoktur) -->
<c-pophover>
  <button>otomatik</button>
  <div slot="pop">Bu bir baloncuk ucu</div>
</c-pophover>

<!-- sabit yön -->
<c-pophover class="left">
  <button>Sol</button>
  <div slot="pop">Bu bir baloncuk ucu</div>
</c-pophover>

<c-pophover class="right">
  <button>Sağ</button>
  <div slot="pop">Bu bir baloncuk ucu</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Aşağı</button>
  <div slot="pop">Bu bir baloncuk ucu</div>
</c-pophover>

<c-pophover class="top">
  <button>üst</button>
  <div slot="pop">Bu bir baloncuk ucu</div>
</c-pophover>
```

## CSS değişkenleri

Özel stiller aşağıdaki CSS değişkenleri aracılığıyla desteklenir:

| değişken adı | varsayılan değer | göstermek |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Kabarcık ipucu ile tetikleyici öğe arasındaki boşluk |
| `--pophover-margin` | `24px` | Uyarlanabilir konumlandırma hesaplaması sırasında görünüm kenarı güvenlik marjı |
| `--pophover-bg-top` | `#18181ce0` | Kabarcık degrade arka plan üst rengi |
| `--pophover-bg-mid` | `#121215e5` | Balonun sol ve sağ taraflarındaki okların arka plan rengi |
| `--pophover-bg-bottom` | `#0c0c0feb` | Kabarcık degrade arka planının alt ve üst ve alt yan oklarının rengi |

## Simge ipucu stili (`.Ico`)

Saf simge tipi kayan bilgi istemi düğmeleri ("Düzenle", "Sil" vb. gibi) için, dış kaba veya bileşenin kendisine `.Ico` sınıf adını ekleyerek birleşik bir simge ve fareyle üzerine gelme stili uygulayabilirsiniz.

### Yapı örneği

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">düzenlemek</div>
  </c-pophover>
</b>
```

Temel düzen, boyut (18x18 piksel) ve fareyle üzerine gelme rengi değişiklikleri `.Ico > c-pophover > a`'de birleştirildi. Geliştiricilerin yalnızca ilgili simge dosyasını ayrı olarak belirtmeleri gerekir; örneğin:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```