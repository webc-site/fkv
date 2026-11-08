# wirtualny pasek przewijania

- **Przewijanie dwukierunkowe**: Zapewnia przewijanie w pionie (`c-vs`) i w poziomie (`c-hs`).
- **Reakcja rozmiaru**: Monitoruj zmiany zawartości i rozmiaru obszaru, automatycznie aktualizuj rozmiar i położenie paska przewijania oraz automatycznie ukrywaj, gdy nie ma przepełnienia.
- **Wsparcie interaktywne**: Obsługuje przewijanie kółkiem, przeciąganie suwaka i pozycjonowanie śledzenia kliknięć.
- **Ukrywanie stanu**: Wyświetlaj i poszerzaj suwak podczas najeżdżania kursorem lub przewijania i automatycznie ukrywaj go, gdy nie poruszasz się.

## przewijanie pionowe

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## przewijanie poziome

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## części stylu

Dostosuj styl wewnętrznych elementów Shadow DOM poprzez `::part()`:

- `::part(scroll)`: Przewiń obszar zawartości.
- `::part(bar)`: Ścieżka paska przewijania.
- `::part(si)`: Suwak paska przewijania.
- `::part(i)`: Wewnętrzna warstwa zawijająca.
- `::part(drag)`: Ścieżka paska przewijania w stanie przeciągania.

## klasa stylu

- `.drag`: Stosowane do elementu `body`, wskazując, że znajduje się on w stanie przewijania poprzez przeciąganie.

## Właściwości niestandardowe

- `--si-bg`: Kolor tła suwaka.
- `--si-anim`: Animacja suwaka.
- `--cursorScrollhSvg`: Kursor myszy po najechaniu poziomym paskiem przewijania.
- `--cursorScrollvSvg`: Kursor myszy po najechaniu pionowym paskiem przewijania.