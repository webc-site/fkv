# Açılır seçimli sekme sayfası

- Yalnızca geçerli sekme sayfası etkinleştirildiğinde görüntülenecek olan uyarlanabilir genişlik açılır seçim kutusunun (`<c-select>`) etikete yerleştirilmesini destekler.
- Yapısal belirtim: Alt öğe, etiket gezinmesi olarak `nav > a` öğesini kullanır ve doğrudan alt öğe `b`, içerik alanı olarak hizmet eder.
- `a[value]` etiketini, `a` `c-select` (grubun değer özelliğine ve seçilen seçenek değerine göre) ve `b[slot]` içerik düğümüyle otomatik olarak ilişkilendirir.
- Şu anda etkin olan öğenin `key` özelliği aracılığıyla otomatik olarak `localStorage` olarak kalıcı hale getirilmesini destekler.
- Her grup için geçmiş alt seçimlerin otomatik olarak kaydedilmesini ve `key + '/' + tab` kullanılarak `localStorage` olarak geri yüklenmesini destekler.
- `value` özelliği aracılığıyla etkin sekmenin belirtilmesini destekler.

## Demoyu kullanın

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>dil</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistem</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript bir betik dilidir</b>
    <b slot="rs">Rust, sistem düzeyinde bir programlama dilidir</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS, Unix tabanlı bir grafik işletim sistemidir</b>
    <b slot="win">Windows, Microsoft tarafından geliştirilen bir işletim sistemidir</b>
  </b>
</c-tab-select>
```

## mülk

- `key`: localStorage kalıcılığı için kullanılan anahtar adı
- `value`: şu anda seçili değer

## Özel etkinlikler

- `change`: Sekmeler arasında geçiş yaparken tetiklenir, `e.value`, o anda etkin olan seçeneğin `value` değeridir.