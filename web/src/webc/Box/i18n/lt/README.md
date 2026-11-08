# Sukurkite ir parodykite iššokantįjį sluoksnį

- Funkcinis skambutis sukurti ir rodyti iššokantįjį sluoksnį
- Uždarius elementus automatiškai pašalinkite elementus iš DOM
- Užkirsti kelią Esc klavišo atšaukimui

## Naudokite demonstracinę versiją

```html
<button>Spustelėkite, kad iššoktumėte</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Pavadinimas";
  description.textContent = "Tekstas";
  button.className = "Btn Main";
  button.textContent = "Gerai";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Sąsajos parametrai

### Numatytosios eksportuotos funkcijos

`Box()`

- **Grąžinama reikšmė**: `HTMLDialogElement`, buvo pridėtas `Box` stiliaus klasės dialogo elementas.

## stiliaus klasė

### `.Box`

Taikoma elementui `dialog` nustatant padėties nustatymą, centravimą ir fono kaukės suliejimą.

### `.Lg`

Stiliaus klasė, taikoma vidiniam turiniui, nustatanti fono spalvą, užapvalintus kampus, šešėlinį ir vertikalų išdėstymą bei apibrėžianti vidinių `h3` ir `p` išdėstymą.