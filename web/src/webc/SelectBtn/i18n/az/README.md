# Açılan seçim qutusu və düymənin birləşməsi

- Seçim qutusunu və düyməni üfüqi şəkildə birləşdirin
- Seçilmiş vəziyyəti avtomatik olaraq localStorage-da yadda saxlayın
- Düymə kliklərini kəsin və fərdi hadisələri göndərin

## Demodan istifadə edin

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Seçim 1</option>
    <option value="option2">Seçim 2</option>
  </c-select>
  <button>qaçmaq</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## əmlak

- `key`: localStorage davamlılığı üçün istifadə edilən açar adı
- `value`: hazırda seçilmiş dəyər

## hadisə

- `submit`: Düymə kliklədikdə göndərilir, `e.value` hazırda seçilmiş dəyəri daşıyır