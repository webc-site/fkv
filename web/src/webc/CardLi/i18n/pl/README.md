# Ładuj i wyświetlaj listę kart asynchronicznie

- **Ładowanie asynchroniczne**: pobieranie danych za pomocą funkcji asynchronicznej
- **Wyświetlanie stanu**: Obsługuje trzy stany: ładowanie, puste dane i karta renderująca.
- **Elastyczny układ**: Karty są ułożone w rzędach

## Skorzystaj z wersji demonstracyjnej

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Tytuł " + i, "Opis " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parametry interfejsu

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funkcja asynchronicznego ładowania danych. Format każdego podklucza to `[nazwa, wprowadzenie, link]`.

## klasa stylu

- `.CardLi`: element zewnętrzny
- `.Card`: element karty
- `.Ing`: Ładowanie animowanego elementu