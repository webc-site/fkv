# Kartenliste asynchron laden und anzeigen

- **Asynchrones Laden**: Daten über eine asynchrone Funktion abrufen
- **Statusanzeige**: Unterstützt drei Zustände: Laden, leere Daten und Rendering-Karte.
- **Flexibles Layout**: Karten sind in Reihen angeordnet

## Nutzen Sie die Demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titel" + i, "Beschreibung" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Schnittstellenparameter

### CardLi(loader)

- **Loader**: `() => Promise<Array<[string, string, string]>>`, asynchrone Datenladefunktion. Das Format jedes Unterschlüssels ist `[Name, Einführung, Link]`.

## Stilklasse

- `.CardLi`: äußeres Element
- `.Card`: Kartenelement
- `.Ing`: Animiertes Element wird geladen