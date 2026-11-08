# Utwórz i wyświetl warstwę wyskakującą z przyciskiem zamykania

- Funkcjonalne wywołanie umożliwiające utworzenie i wyświetlenie warstwy wyskakującej
- Wbudowany przycisk zamykania, kliknij, aby zamknąć i usunąć element z DOM
- Naciśnij klawisz Esc, aby rozmyć pole wprowadzania lub zamknąć warstwę wyskakującą

## Skorzystaj z wersji demonstracyjnej

```html
<button>Kliknij, aby wyskoczyć</button>

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


## Parametry interfejsu

Domyślna funkcja eksportu zwraca `HTMLDialogElement`, a do elementu okna dialogowego dodano klasy stylu `Box` i `X`.

## klasa stylu

### `.Box.X`

Zastosowany do elementu `dialog` określa styl okna dialogowego.

### `a.X`

Zastosowany do elementu `a` określa styl przycisku zamykania.