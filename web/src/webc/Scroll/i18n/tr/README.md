# sanal kaydırma çubuğu

- **İki yönlü kaydırma**: Dikey (`c-vs`) ve yatay (`c-hs`) kaydırma sağlar.
- **Boyut yanıtı**: İçerik ve alan boyutu değişikliklerini izleyin, kaydırma çubuğu boyutunu ve konumunu otomatik olarak güncelleyin ve taşma olmadığında otomatik olarak gizleyin.
- **Etkileşimli destek**: Tekerlek kaydırmayı, kaydırıcıyı sürüklemeyi ve tıklama konumlandırmasını izlemeyi destekler.
- **Durum Gizleme**: Fareyle üzerine gelindiğinde veya kaydırıldığında kaydırıcıyı gösterip genişletin ve sabit durduğunda otomatik olarak gizleyin.

## dikey kaydırma

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## yatay kaydırma

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## stil parçaları

`::part()` aracılığıyla Shadow DOM dahili öğelerinin stilini özelleştirin:

- `::part(scroll)`: İçerik alanını kaydırın.
- `::part(bar)`: Kaydırma çubuğu izi.
- `::part(si)`: Kaydırma çubuğu kaydırıcısı.
- `::part(i)`: Dahili sarma katmanı.
- `::part(drag)`: Sürükleme durumundaki kaydırma çubuğu izi.

## stil sınıfı

- `.drag`: `body` öğesine uygulanır ve sürükleyerek kaydırma durumunda olduğunu belirtir.

## Özel özellikler

- `--si-bg`: Kaydırıcının arka plan rengi.
- `--si-anim`: Kaydırıcı animasyonu.
- `--cursorScrollhSvg`: Yatay kaydırma çubuğu üzerine gelindiğinde fare imleci.
- `--cursorScrollvSvg`: Dikey kaydırma çubuğu üzerine gelindiğinde fare imleci.