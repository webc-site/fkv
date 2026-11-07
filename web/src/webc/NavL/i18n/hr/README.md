# Automatski promijeni navigacijski izbornik i gumb za povratak

- Automatski prelomi podređene elemente za generiranje navigacijskih izbornika i struktura gumba za povratak
- Automatski pratite povijest rutiranja i izračunajte prethodni povratni put
- Prikažite navigacijski izbornik kada se ruta podudara sa stavkom izbornika, inače prikažite gumb Natrag

## Koristite demo

```html
<c-nav-l>
  <a href="/">naslovna stranica</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## stilska klasa

- `B`: aktivira stanje gumba za povratak. Prikaži gumb za povratak i sakrij navigacijski izbornik. Automatski se prebacuje unutar komponente na temelju usmjeravanja.