# Değiştirilebilir sekmeler

- Etiketin seçilen durumu aşağıdaki içerik alanıyla sorunsuz bir şekilde entegre edilmiştir.
- Yapısal özellikler: Alt öğeler, etiket gezinmesi olarak `nav > a` kullanacak şekilde sabitlenir ve içerik alanı olarak doğrudan alt öğe `b` kullanılır.
- `a[value]` etiketini otomatik olarak `b[slot]` içerik düğümüyle ilişkilendirin.
- Geçerli etkin sayfanın `key` özelliği aracılığıyla otomatik olarak `localStorage` olarak kalıcı hale getirilmesini destekler.
- Etkinleştirilmiş sekme sayfasının `value` özelliği aracılığıyla belirtilmesini destekler (önbellek yoksa ve `value` varsa, `class="A"` içeren etiket veya ilk etiket varsayılan olarak seçilecektir).
- Hafif Hafif DOM uygulaması, Gölge DOM engeli yok, stil özelleştirmesinde yüksek derecede özgürlük.

## Demoyu kullanın

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">elma</a>
    <a value="banana">muz</a>
    <a value="orange">mandalina</a>
  </nav>
  <b slot="apple">Elma besleyici bir meyvedir</b>
  <b slot="banana">Muz tropik bir meyvedir</b>
  <b slot="orange">Portakal C vitamini açısından zengindir</b>
</c-tab>
```

## mülk

- `key`: localStorage kalıcılığı için kullanılan anahtar adı
- `value`: şu anda seçili değer

## Özel etkinlikler

- `change`: Sekme değiştirildiğinde tetiklenen `e.value`, o anda etkin olan etiketin `value` değeridir.