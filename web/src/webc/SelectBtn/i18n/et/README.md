# Rippmenüü valikukasti ja nupu kombinatsioon

- Ühendage valikukast ja nupp horisontaalselt
- Salvestage valitud olek automaatselt kohalikku salvestusruumi
- Peatage nuppude klõpsud ja saatke kohandatud sündmusi

## Kasutage demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">1. võimalus</option>
    <option value="option2">2. variant</option>
  </c-select>
  <button>jooksma</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## vara

- `key`: kohaliku salvestusruumi püsivuse tagamiseks kasutatav võtme nimi
- `value`: praegu valitud väärtus

## sündmus

- `submit`: saadetakse nupule klõpsamisel, `e.value` kannab praegu valitud väärtust