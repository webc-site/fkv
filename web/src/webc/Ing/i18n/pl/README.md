# Wskaźnik stanu załadunku lub przetwarzania

- Ujednolica kursor w stan oczekiwania
- Pokazuje animację pulsującą wskazującą stan przetwarzania
- Zastosuj maskę rozmycia do formularza, aby zapobiec duplikowaniu zgłoszeń

## Skorzystaj z wersji demonstracyjnej

```html
<!-- niezależny wskaźnik załadunku -->
<div class="Ing"></div>

<!-- Ładowanie formularza z maską -->
<form class="Ing">
  <input type="text" placeholder="Pole wprowadzania">
  <button type="submit">składać</button>
</form>
```

## klasa stylu

### `.Ing`
- Stan wskaźnika to `wait`.
- Pseudoelement `::before` renderuje animację falowania.
- Użyty w elemencie `form` pseudoelement `::after` renderuje maskę rozmycia o wielkości 1 piksela, uniemożliwiając interakcję i zaznaczanie tekstu.