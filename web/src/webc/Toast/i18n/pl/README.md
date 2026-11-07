# Wyskakujące okienko podpowiedzi na krawędzi strony

- **Wywołanie funkcji**: Zapewnia interfejsy normalne i komunikaty o błędach
- **Automatyczny stos**: Wiele końcówek automatycznie oblicza odstępy i układa w pionie
- **Zaplanowane i ręczne wyłączanie**: Obsługuje zaplanowane i ręczne wyłączanie

## Skorzystaj z wersji demonstracyjnej

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Treść podpowiedzi";
});

toastErr((el) => {
  el.textContent = "Treść błędu";
});

toast(
  (el) => {
    el.textContent = "Zamknij automatycznie po 99 sekundach";
  },
  99
);
```

## Parametry interfejsu

### toast(render, timeout)

Twórz i wyświetlaj podpowiedzi.

- `render`: `(el) => void`, renderowanie wywołania zwrotnego, `el` to element okna zachęty.
- `timeout`: `Number`, limit czasu w sekundach. Domyślnie 9. Ustaw na 0, aby nie wyłączać się automatycznie.
- Wartość zwracana: element okna zachęty. Obsługuje `el.close()` wyłączone.

### toastErr(render, timeout)

Twórz i wyświetlaj komunikaty o błędach. Parametry są takie same jak w `toast`, z klasą stylu `.ERR`.

## klasa stylu

- `.Toast`: Klasa bazowa okna podpowiedzi.
- `.ERR`: Stan błędu.
- `.animated` / `.fadeInR` / `.fadeOutR`: animowane przejście.
- `.x`: Przycisk zamykania.