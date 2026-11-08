# Farklı ekran boyutlarına uyarlanmış gezinme menüsü

- Masaüstü: Bağlantılar yatay döşemelerde görüntülenir
- Mobil sürüm: Menü düğmesine daraltın, kenar çubuğu çekmecesini dışarı kaydırmak için tıklayın
- Rota aktivasyonu: mevcut yolu ve karma değerini otomatik olarak karşılaştırın ve eşleşen bağlantıya aktivasyon durumu sınıfı adını ekleyin `A`
- Otomatik olarak kapat: Mobil terminal bir bağlantıya veya kenar çubuğunun harici arka planına tıkladığında kenar çubuğu otomatik olarak kapanır.

## kullanmak

```html
<c-nav-menu>
  <a href="/">ön sayfa</a>
  <a href="/doc">belge</a>
  <a href="/about">hakkında</a>
</c-nav-menu>
```

## göstermek

Bileşen başlatıldığında, alt öğede `c-side` yoksa, mobil menü düğmesi ve `c-side` kenar çubuğu otomatik olarak oluşturulacak ve tüm `a` etiketleri kenar çubuğuna taşınacaktır.

## stil sınıfı

- **A**: Etkinleştirilen bağlantı otomatik olarak `A` sınıf adını ekleyecektir.