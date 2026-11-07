# Efekt podświetlenia tła i krawędzi z matowego szkła

Półprzezroczyste obramowanie i wewnętrzny cień tworzą mikrowypukłe podkreślenia krawędzi.
Jako podstawowa zależność stylu komponentów przycisku i pola wprowadzania.

## Skorzystaj z wersji demonstracyjnej

```html
<main class="Lg">
  <article>
    <h3>tytuł</h3>
    <pre>Tekst treści</pre>
  </article>
</main>
```

## klasa stylu

### `.Lg`
Podstawowy styl karty. Zastosuj `backdrop-filter` rozmycie, `border` mikrowytłoczone obramowanie i `box-shadow` cień wewnętrzny. Zastosuj filtr `svg/glass.svg#h` po najechaniu kursorem.

### `main.Lg`
Główny kontener blokowy strony. Zaokrąglone rogi `24px` z responsywnym dopełnieniem (`16px` jeśli szerokość jest mniejsza niż `800px`, `24px` w przeciwnym razie).

### `.Btn`, `.BtnC`, `.Input`
Nazwa klasy podstawowego komponentu. Po wprowadzeniu tego stylu automatycznie uzyskasz elastyczny układ, hierarchiczne zależności i podstawowe właściwości matowego szkła.