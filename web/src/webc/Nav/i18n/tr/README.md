# Sayfa kaydırıldığında otomatik olarak daraltılan veya genişleyen üst gezinme çubuğu

- **Otomatik Gizle**: Sayfayı aşağı kaydırırken otomatik olarak gizler.
- **Otomatik genişletme**: Sayfa yukarı kaydırıldığında veya kaydırma çubuğu kaybolduğunda otomatik olarak görüntülenir.
- **Performans Optimizasyonu**: Kaydırmayı kısıtlamak için `requestAnimationFrame` kullanın.
- **Kaynakları serbest bırakın**: Etkinlik dinleyicileri ve gözlemcileri, bileşen yok edildiğinde otomatik olarak kaldırılır.

## Demoyu kullanın

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Bağlantı</a>
      </c-nav>
    </nav>
    <div class="content">Gezinmeyi gizlemek için aşağı kaydırın, göstermek için yukarı kaydırın</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Kullanım talimatları

`<c-vs>` öğesinin alt öğesi veya alt öğesi olarak kullanılmalıdır.

## stil durumu

Bileşen, `nav.Nav` üzerinde aşağıdaki sınıf adını değiştirir:

- `D`: Durumu gösterir (aşağı veya yukarı doğru kaydırın).
- `I`: Geçiş durumu gizleniyor.
- `H`: Gizli durum.

## stil değişkenleri

- `--top`: Konumlandırma ve geçiş için kullanılan, bileşen tarafından otomatik olarak negatif bir yükseklik değerine güncellenir.