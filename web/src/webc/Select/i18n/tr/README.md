# Açılan seçim, genişlik geçerli öğeye uyarlanır

Yerel `select` genişliği varsayılan olarak en uzun seçenek metnine göre belirlenir. Kısa seçenekleri değiştirirken çok fazla beyaz alan oluşuyor ve bu da görünümü etkiliyor.

Bu bileşen, `b` etiketinin üzerindeki yerel `select` öğesini şeffaf bir şekilde kaplayarak genişliğin, seçili metnin uzunluğuna doğru şekilde uyum sağlamasına olanak tanır.

- Tüm terminallerde yerel `select` açılır menü stillerini ve erişilebilirlik desteğini koruyun
- Tam klavye Sekmesi gezinmesi ve yön tuşu değiştirme gibi yerel etkileşimli işlemleri destekler.
- Fazla beyaz alanı ortadan kaldırmak için genişliği mevcut seçeneğe uyarlayın

## Demoyu kullanın

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```