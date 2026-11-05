# Kaartlijst asynchroon laden en weergeven

- **Asynchroon laden**: gegevens ophalen via een asynchrone functie
- **Statusweergave**: Ondersteunt drie statussen: laden, lege gegevens en kaart weergeven.
- **Flexibele indeling**: kaarten zijn in rijen gerangschikt

## Gebruik de demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titel " + i, "Beschrijving" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interfaceparameters

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, functie voor het asynchrone laden van gegevens. Het formaat van elke subsleutel is `[naam, introductie, link]`.

## stijl klasse

- `.CardLi`: buitenste element
- `.Card`: kaartelement
- `.Ing`: geanimeerd element laden