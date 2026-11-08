# Nolaižamā izvēles lodziņa un pogas kombinācija

- Savienojiet atlases lodziņu un pogu horizontāli
- Automātiski saglabājiet atlasīto stāvokli vietējā krātuvē
- Pārtveriet pogu klikšķus un nosūtiet pielāgotus notikumus

## Izmantojiet demonstrāciju

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">1. iespēja</option>
    <option value="option2">2. iespēja</option>
  </c-select>
  <button>palaist</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## īpašums

- `key`: atslēgas nosaukums, kas tiek izmantots vietējās krātuves noturībai
- `value`: pašlaik atlasītā vērtība

## notikumu

- `submit`: tiek nosūtīts, kad tiek noklikšķināts uz pogas, `e.value` satur pašlaik atlasīto vērtību