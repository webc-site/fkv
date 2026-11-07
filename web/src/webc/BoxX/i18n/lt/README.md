# Sukurkite ir parodykite iššokantįjį sluoksnį uždarymo mygtuku

- Funkcinis skambutis sukurti ir rodyti iššokantįjį sluoksnį
- Integruotas uždarymo mygtukas, spustelėkite, kad uždarytumėte ir pašalintumėte elementą iš DOM
- Paspauskite Esc, kad sufokusuotumėte įvesties laukelį arba uždarytumėte iššokantįjį sluoksnį

## Naudokite demonstracinę versiją

```html
<button>Spustelėkite, kad iššoktumėte</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "tekstas";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Sąsajos parametrai

Numatytoji eksportavimo funkcija grąžina `HTMLDialogElement`, o `Box` ir `X` stiliaus klasės buvo pridėtos prie dialogo elemento.

## stiliaus klasė

### `.Box.X`

Taikoma `dialog` elementui, nurodo dialogo stilių.

### `a.X`

Taikoma `a` elementui, nurodo uždarymo mygtuko stilių.