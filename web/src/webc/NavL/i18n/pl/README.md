# Automatycznie przełączaj menu nawigacyjne i przycisk powrotu

- Automatycznie zawijaj elementy podrzędne, aby wygenerować menu nawigacyjne i struktury przycisków powrotu
- Automatycznie śledź historię tras i obliczaj poprzednią ścieżkę powrotną
- Wyświetl menu nawigacji, gdy trasa odpowiada pozycji menu, w przeciwnym razie wyświetl przycisk Wstecz

## Skorzystaj z wersji demonstracyjnej

```html
<c-nav-l>
  <a href="/">strona tytułowa</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## klasa stylu

- `B`: aktywuje stan przycisku powrotu. Pokaż przycisk Wstecz i ukryj menu nawigacyjne. Automatycznie przełączane w komponencie w oparciu o routing.