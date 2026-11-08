# Utwórz i wyświetl warstwę wyskakującą

- Funkcjonalne wywołanie umożliwiające utworzenie i wyświetlenie warstwy wyskakującej
- Automatycznie usuwaj elementy z DOM po zamknięciu
- Zapobiegaj anulowaniu klawisza Esc

## Skorzystaj z wersji demonstracyjnej

```html
<button>Kliknij, aby wyskoczyć</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  tytuł.textContent = "Tytuł";
  opis.textContent = "Tekst";
  button.className = "Btn Main";
  przycisk.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametry interfejsu

### Domyślne eksportowane funkcje

`Box()`

- **Wartość zwracana**: `HTMLDialogElement`, dodano element okna dialogowego klasy stylu `Box`.

## klasa stylu

### `.Box`

Dotyczy elementu `dialog` umożliwiającego ustawienie pozycjonowania, centrowania i rozmycia maski tła.

### `.Lg`

Klasa stylu stosowana do zawartości wewnętrznej, ustawiająca kolor tła, zaokrąglone rogi, cień i układ pionowy oraz definiująca układ wewnętrznych `h3` i `p` .