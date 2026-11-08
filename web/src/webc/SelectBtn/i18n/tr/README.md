# Açılır seçim kutusu ve düğmenin birleşimi

- Seçim kutusunu ve düğmeyi yatay olarak birleştirme
- Seçilen durumu otomatik olarak localStorage'a kaydet
- Düğme tıklamalarını engelleyin ve özel etkinlikleri gönderin

## Demoyu kullanın

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Seçenek 1</option>
    <option value="option2">Seçenek 2</option>
  </c-select>
  <button>koşmak</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## mülk

- `key`: localStorage kalıcılığı için kullanılan anahtar adı
- `value`: şu anda seçili değer

## etkinlik

- `submit`: Düğmeye tıklandığında gönderilir, `e.value` o anda seçili olan değeri taşır