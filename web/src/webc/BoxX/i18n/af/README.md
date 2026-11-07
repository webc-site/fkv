# Skep en vertoon 'n opspringlaag met 'n toemaakknoppie

- Funksionele oproep om opspringlaag te skep en te vertoon
- Ingeboude toemaakknoppie, klik om toe te maak en die element van die DOM te verwyder
- Druk Esc om die invoerkassie te ontfokus of die opspringlaag toe te maak

## Gebruik die demo

```html
<button>Klik om te verskyn</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "teks";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Interface parameters

Die verstek uitvoerfunksie gee `HTMLDialogElement` terug, en die `Box` en `X` stylklasse is by die dialoogelement gevoeg.

## styl klas

### `.Box.X`

Toegepas op die `dialog` element, spesifiseer die dialoogstyl.

### `a.X`

Toegepas op die `a`-element, spesifiseer die sluitknoppiestyl.