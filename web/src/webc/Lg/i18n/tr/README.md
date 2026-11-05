# Buzlu cam arka plan ve kenar vurgulama efekti

Yarı şeffaf kenarlık ve iç gölge, mikro rölyefli kenar vurguları oluşturur.
Düğme ve giriş kutusu bileşenlerinin temel stil bağımlılığı olarak.

## Demoyu kullanın

```html
<main class="Lg">
  <article>
    <h3>başlık</h3>
    <pre>İçerik metni</pre>
  </article>
</main>
```

## stil sınıfı

### `.Lg`
Kartın temel stili. `backdrop-filter` bulanıklaştırma, `border` mikro kabartmalı kenarlık ve `box-shadow` iç gölge uygulayın. Fareyle üzerine gelindiğinde `svg/glass.svg#h` filtresini uygulayın.

### `main.Lg`
Sayfanın ana blok kapsayıcısı. Duyarlı dolgulu yuvarlatılmış köşeler `24px` (genişlik `800px`'den küçükse `16px`, aksi halde `24px`).

### `.Btn`, `.BtnC`, `.Input`
Temel bileşen sınıfı adı. Bu stili uyguladıktan sonra buzlu camın esnek düzenini, hiyerarşik ilişkilerini ve temel özelliklerini otomatik olarak elde edeceksiniz.