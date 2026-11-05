# Wybór rozwijany, szerokość dostosowuje się do bieżącego elementu

Szerokość natywnego `select` jest domyślnie określana przez najdłuższy tekst opcji. Podczas przełączania krótkich opcji jest zbyt dużo białej przestrzeni, co wpływa na wygląd.

Komponent ten w przezroczysty sposób zakrywa natywny znacznik `select` nad znacznikiem `b`, umożliwiając dokładne dostosowanie szerokości do długości aktualnie zaznaczonego tekstu.

- Zachowaj natywne `select` style menu rozwijanych i obsługę ułatwień dostępu na wszystkich terminalach
- Obsługuje natywne operacje interaktywne, takie jak pełna nawigacja na klawiaturze i przełączanie klawiszy kierunkowych.
- Dostosuj szerokość do bieżącej opcji, aby wyeliminować nadmiar białych znaków

## Skorzystaj z wersji demonstracyjnej

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```