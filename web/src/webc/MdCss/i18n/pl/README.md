# Style typografii zoptymalizowane pod kątem czytania

- **Podstawowy skład**: Dostosuj do tytułów, akapitów, pogrubienia, kursywy, przekreślenia, kodu wbudowanego, obrazów i linii podziału.
- **Animacja łącza**: Wyświetla animację podkreślenia po najechaniu myszką.
- **Lista projektów**: Listy nieuporządkowane używają punktorów, listy uporządkowane używają zaokrąglonych numerów seryjnych w tle i obsługują zagnieżdżanie wielopoziomowe.
- **Lista zadań**: Zapewnia styl pola wyboru, obsługuje status zadań i ukończonych.
- **Wskazówki ostrzegawcze**: Obsługuje pięć typów okien ostrzegawczych: Uwaga, Wskazówka, Ważne, Ostrzeżenie i Uwaga.
- **Tabele i kod**: Zapewnia podświetlanie wierszy tabeli po najechaniu myszką oraz bloki kodu z cieniami i gradientowym tłem.

## Skorzystaj z wersji demonstracyjnej

```html
<div class="Md">
  <h1>tytuł</h1>
  <p>Tekst akapitu i <a href="#">link</a></p>
  <hr>
  <h2>Tytuł drugiego poziomu</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Element zadania</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Treść notatki</p>
  </blockquote>
</div>
```

## klasa stylu

Style są stosowane do elementów HTML w kontenerze `.Md`.

### Styl okna alertu

Dodaj następującą nazwę klasy w elemencie `blockquote` z nazwą klasy `.q`, aby przełączyć okno alertu:

- `note`: Notatki
- `tip`: Wskazówka
- `important`: Ważne
- `warning`: ostrzeżenie
- `caution`: Uwaga