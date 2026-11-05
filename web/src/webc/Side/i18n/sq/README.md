# Panelet e sirtarëve që rrëshqasin nga ana

- Krijoni automatikisht struktura të brendshme lëvizëse
- Klikoni zonën e jashtme për t'u shembur automatikisht
- Rrëshqitni gjestin majtas dhe djathtas për ta mbyllur
- Pozicionimi me rrëshqitje anësore në drejtimet majtas dhe djathtas
- Siguroni ngjarje të hapura dhe të mbyllura

## Përdorni demonstrimin

```html
<button class="open-btn">Hapur</button>

<c-side class="right">
  <div>Përmbajtja e sirtarit</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## ndërfaqe

### metodë

- `open(el)`: Funksioni i eksportit, sirtari i hapur
- `close(el)`: Funksioni i eksportit, mbyllja e sirtarit
- `el.open()`: metodë shembulli, sirtar i hapur
- `el.close()`: metoda e shembullit, mbyllni sirtarin

### ngjarje

- `open`: Ndizet kur ndizet
- `close`: Zjarret kur mbyllet

## klasë stili

- `.right`: pozicionimi me rrëshqitje djathtas
- `.top`: rrëshqitni jashtë pozicionimit
- `.bottom`: rrëshqitni nga pozicionimi
- (Nëse nuk është caktuar, parazgjedhja është të rrëshqasë majtas)
- `.open`: statusi i hapur
- `.H`: gjendje e fshehur
- `.T`: gjendja e tranzicionit të animacionit