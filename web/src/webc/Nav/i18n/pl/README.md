# Górny pasek nawigacyjny, który automatycznie zwija się lub rozwija podczas przewijania strony

- **Automatyczne ukrywanie**: Automatyczne ukrywanie podczas przewijania strony w dół.
- **Automatyczne rozwinięcie**: Wyświetlane automatycznie po przewinięciu strony w górę lub zniknięciu paska przewijania.
- **Optymalizacja wydajności**: Użyj `requestAnimationFrame`, aby ograniczyć przewijanie.
- **Zwolnij zasoby**: Detektory i obserwatorzy zdarzeń są automatycznie usuwani po zniszczeniu komponentu.

## Skorzystaj z wersji demonstracyjnej

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Połączyć</a>
      </c-nav>
    </nav>
    <div class="content">Przesuń w dół, aby ukryć nawigację, przesuń w górę, aby ją wyświetlić</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Instrukcje użytkowania

Musi być używany jako element podrzędny lub potomny elementu `<c-vs>`.

## stan stylu

Komponent przełącza następującą nazwę klasy na `nav.Nav`:

- `D`: Pokaż status (przesuń w dół lub na górę).
- `I`: Ukrywanie stanu przejścia.
- `H`: Stan ukryty.

## zmienne stylu

- `--top`: Automatycznie aktualizowana przez komponent do ujemnej wartości wysokości, używanej do pozycjonowania i przejścia.