# Kreirajte i prikažite iskačući sloj sa dugmetom za zatvaranje

- Funkcionalni poziv za kreiranje i prikaz popup sloja
- Ugrađeno dugme za zatvaranje, kliknite da zatvorite i uklonite element iz DOM-a
- Pritisnite Esc da defokusirate okvir za unos ili zatvorite iskačući sloj

## Koristite demo

```html
<button>Kliknite da se pojavi</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "tekst";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parametri interfejsa

Zadana funkcija izvoza vraća `HTMLDialogElement`, a stilske klase `Box` i `X` su dodate elementu dijaloga.

## stilska klasa

### `.Box.X`

Primijenjeno na `dialog` element, specificira stil dijaloga.

### `a.X`

Primijenjeno na `a` element, specificira stil gumba za zatvaranje.