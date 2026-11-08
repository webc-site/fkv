# ડ્રોપ-ડાઉન પસંદગી બોક્સ અને બટનનું સંયોજન

- પસંદગી બોક્સ અને બટનને આડી રીતે વિભાજિત કરો
- પસંદ કરેલ રાજ્યને સ્થાનિક સ્ટોરેજમાં આપમેળે સાચવો
- ઇન્ટરસેપ્ટ બટન ક્લિક્સ અને કસ્ટમ ઇવેન્ટ્સ મોકલો

## ડેમોનો ઉપયોગ કરો

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">વિકલ્પ 1</option>
    <option value="option2">વિકલ્પ 2</option>
  </c-select>
  <button>દોડવું</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## મિલકત

- `key`: લોકલ સ્ટોરેજ દ્રઢતા માટે વપરાયેલ કી નામ
- `value`: હાલમાં પસંદ કરેલ મૂલ્ય

## ઘટના

- `submit`: જ્યારે બટન પર ક્લિક કરવામાં આવે ત્યારે મોકલવામાં આવે છે, `e.value` હાલમાં પસંદ કરેલ મૂલ્ય ધરાવે છે