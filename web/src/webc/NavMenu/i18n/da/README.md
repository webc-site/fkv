# Navigationsmenu tilpasset forskellige skærmstørrelser

- Desktop: Links vises i vandrette fliser
- Mobilversion: Skjul ind i menuknappen, klik for at glide ud af skuffen i sidepanelet
- Ruteaktivering: Sammenlign automatisk den aktuelle sti og hashværdi, og tilføj aktiveringsstatusklassenavnet til det matchende link `A`
- Luk automatisk: Når mobilterminalen klikker på et link eller den eksterne baggrund på sidebjælken, lukkes sidebjælken automatisk.

## bruge

```html
<c-nav-menu>
  <a href="/">forsiden</a>
  <a href="/doc">dokument</a>
  <a href="/about">om</a>
</c-nav-menu>
```

## illustrere

Når komponenten er initialiseret, hvis der ikke er nogen `c-side` i det underordnede element, vil mobilmenuknappen og `c-side` sidebjælke automatisk blive oprettet, og alle `a` tags vil blive flyttet ind i sidebjælken.

## stil klasse

- **A**: Det aktiverede link tilføjer automatisk klassenavnet `A`