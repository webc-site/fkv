# Samodejno preklopi navigacijski meni in gumb za vrnitev

- Samodejno ovijte podrejene elemente, da ustvarite navigacijski meni in strukture gumbov za vrnitev
- Samodejno izsledite zgodovino poti in izračunajte prejšnjo povratno pot
- Prikažite navigacijski meni, ko se pot ujema z menijsko postavko, sicer prikažite gumb za nazaj

## Uporabite demo

```html
<c-nav-l>
  <a href="/">sprednja stran</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## stilski razred

- `B`: aktivira stanje gumba za vrnitev. Pokaži gumb za nazaj in skrij navigacijski meni. Samodejno preklaplja znotraj komponente na podlagi usmerjanja.