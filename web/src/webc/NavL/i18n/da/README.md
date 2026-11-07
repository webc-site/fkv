# Skift automatisk navigationsmenu og returknap

- Indpak automatisk underordnede elementer for at generere navigationsmenu og returnere knapstrukturer
- Spor automatisk rutehistorik og beregn den forrige retursti
- Vis navigationsmenuen, når ruten matcher menupunktet, ellers vis tilbage-knappen

## Brug demoen

```html
<c-nav-l>
  <a href="/">forsiden</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## stil klasse

- `B`: aktiverer tilstanden returknap. Vis tilbage-knappen og skjul navigationsmenuen. Skifter automatisk i komponenten baseret på routing.