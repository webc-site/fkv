# Carica e visualizza l'elenco delle carte in modo asincrono

- **Caricamento asincrono**: ottieni dati tramite una funzione asincrona
- **Visualizzazione dello stato**: supporta tre stati: caricamento, dati vuoti e scheda di rendering.
- **Layout flessibile**: le carte sono disposte in righe

## Utilizza la demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titolo " + i, "Descrizione " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parametri dell'interfaccia

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funzione di caricamento dati asincrono. Il formato di ciascuna sottochiave è `[nome, introduzione, collegamento]`.

## lezione di stile

- `.CardLi`: elemento esterno
- `.Card`: elemento della carta
- `.Ing`: Caricamento dell'elemento animato