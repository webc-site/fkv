# Bileşenleri veya araç işlevlerini işlemek için belgeler ve kaynak kodu

- Belgelerin solda ve kaynak kodunun sağda görüntülendiği iki sütunlu duyarlı düzen
- Dokümantasyon ve kaynak kodu, kaynak adrese atlamak için Düzenleme ve Kodlama kısayol düğmelerini destekler.
- Başlık ve giriş üstte görüntülenerek i18n otomatik çevirisini destekler.

## Demoyu kullanın

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Özellik parametreleri

- `src`: Kaynak kodun URL adresi.
- `href`: Belge işaretlemesinin URL adresi, geçerli dile uyum sağlamak için `{code}` yer tutucusunun kullanılmasını destekler.