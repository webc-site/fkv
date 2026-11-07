# Yükleme veya işleme durumu göstergesi

- İmleci bekleme durumuna birleştir
- İşleme durumunu belirtmek için dalgalanma animasyonunu gösterir
- Yinelenen gönderimleri önlemek için forma bulanıklık maskesi uygulama

## Demoyu kullanın

```html
<!-- bağımsız yükleme göstergesi -->
<div class="Ing"></div>

<!-- Form maskeyle yükleniyor -->
<form class="Ing">
  <input type="text" placeholder="Giriş kutusu">
  <button type="submit">göndermek</button>
</form>
```

## stil sınıfı

### `.Ing`
- İşaretçi durumu: `wait`.
- Sözde öğe `::before` bir dalgalanma animasyonu oluşturur.
- `form` öğesinde kullanıldığında, `::after` sözde öğesi, 1 piksellik bir bulanıklık maskesi oluşturarak etkileşimi ve metin seçimini devre dışı bırakır.