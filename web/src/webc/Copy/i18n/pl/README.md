# Skopiuj tekst do schowka

Kliknij przycisk, aby skopiować tekst określonego elementu.

- Obsługuje użycie rel do określenia selektora elementu, który ma zostać skopiowany
- Jeśli nie określono, poprzedni element rodzeństwa jest domyślnie kopiowany.
- Zagnieżdżone okno dialogowe typu popover
- Wsparcie międzynarodowe

## Skorzystaj z wersji demonstracyjnej

```html
<!-- Skopiuj określone elementy -->
<span id="target">Oto tekst do skopiowania</span>
<c-copy rel="#target"></c-copy>

<!-- Domyślnie kopiowane są elementy poprzedzające -->
<span>tekst poprzedniego elementu</span>
<c-copy></c-copy>
```

## parametr

| nieruchomość | zilustrować |
| --- | --- |
| rel | Skopiuj selektor elementu docelowego. Fakultatywny |

## styl statusu

- `c-pophover.Done`: styl okna zachęty po pomyślnym skopiowaniu