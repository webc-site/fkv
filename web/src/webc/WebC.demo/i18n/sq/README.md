# Paraqitni dhe shfaqni demonstrimet e komponentëve, dokumentacionin dhe udhëzimet e instalimit

- Ekstraktoni dhe analizoni titullin, hyrjen dhe përmbajtjen e dokumentit README
- Sandbox për të ekzekutuar demonstrime interaktive të komponentëve dhe për të izoluar stilet
- Shndërroni blloqet e kodeve në dokumente në prezantime interaktive
- Struktura përshtatëse, shfaqja e kolonës me ekran të gjerë, shfaqja e ndërrimit të skedës së ekranit të ngushtë

## Përdorni demonstrimin

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializoni dhe merrni ndërfaqen e funksionimit
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametrat e ndërfaqes

### `WebCDemo(el)`

Inicializoni komponentin.

- **parametër**
  - `el`:`HTMLElement`, kontejner komponenti.
- **Vlera e kthimit**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Konfiguro modulin demo.
  - `setMd(name, md, lang)`: Vendosni emrin e komponentit, përmbajtjen e dokumentit README dhe gjuhën aktuale.
  - `unMount()`: Shkarkoni komponentin dhe pastroni dëgjuesin e ngjarjes.

### Moduli demonstrues atribut `mod`

- `CSS`:`string`, shtegu i fletës së stilit të komponentit.
- `HTM`:`string`, shablloni HTML i komponentit.
- `default`:`function(root)`, funksioni i inicializimit të komponentit, merr kontejnerin e rrënjës së hijes.

## klasë stili

### `.M`

zona e përmbajtjes.

### `.L`

Paneli i majtë përmban udhëzime dhe dokumentacion.

### `.R`

Paneli i djathtë, që përmban demonstrimin.

### `c-nav`

Shiriti i lundrimit në ekran të ngushtë.