# Gezinme menüsünü ve geri dönüş düğmesini otomatik olarak değiştir

- Gezinme menüsü oluşturmak ve düğme yapılarını döndürmek için alt öğeleri otomatik olarak sarın
- Rota geçmişini otomatik olarak izleyin ve önceki dönüş yolunu hesaplayın
- Rota menü öğesiyle eşleştiğinde navigasyon menüsünü görüntüleyin, aksi takdirde geri düğmesini görüntüleyin

## Demoyu kullanın

```html
<c-nav-l>
  <a href="/">ön sayfa</a>
  <a href="/doc">belge</a>
</c-nav-l>
```

## stil sınıfı

- `B`: geri dönüş butonu durumunu etkinleştirir. Geri düğmesini gösterin ve gezinme menüsünü gizleyin. Yönlendirmeye bağlı olarak bileşen içinde otomatik olarak geçiş yapılır.