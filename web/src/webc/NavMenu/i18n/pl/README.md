# Menu nawigacyjne dostosowane do różnych rozmiarów ekranów

- Pulpit: Linki są wyświetlane w poziomych kafelkach
- Wersja mobilna: przycisk Zwiń do menu, kliknij, aby wysunąć szufladę na pasku bocznym
- Aktywacja trasy: automatycznie porównaj bieżącą ścieżkę i wartość skrótu oraz dodaj nazwę klasy statusu aktywacji do pasującego linku `A`
- Automatycznie zamykaj: Kiedy terminal mobilny kliknie łącze lub zewnętrzne tło paska bocznego, pasek boczny zostanie automatycznie zamknięty.

## używać

```html
<c-nav-menu>
  <a href="/">strona tytułowa</a>
  <a href="/doc">dokument</a>
  <a href="/about">o</a>
</c-nav-menu>
```

## zilustrować

Po zainicjowaniu komponentu, jeśli w elemencie podrzędnym nie ma `c-side`, przycisk menu mobilnego i pasek boczny `c-side` zostaną automatycznie utworzone, a wszystkie tagi `a` zostaną przeniesione do paska bocznego.

## klasa stylu

- **A**: Aktywowany link automatycznie doda nazwę klasy `A`