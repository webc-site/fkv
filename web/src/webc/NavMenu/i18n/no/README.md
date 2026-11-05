# Navigasjonsmeny tilpasset ulike skjermstørrelser

- Skrivebord: Koblinger vises i horisontale fliser
- Mobilversjon: Skjul inn i menyknappen, klikk for å skyve ut sidepanelskuffen
- Ruteaktivering: sammenlign automatisk gjeldende bane og hash-verdi, og legg til aktiveringsstatusklassenavn til den samsvarende lenken `A`
- Lukk automatisk: Når mobilterminalen klikker på en lenke eller den eksterne bakgrunnen til sidefeltet, lukkes sidefeltet automatisk.

## bruk

```html
<c-nav-menu>
  <a href="/">forsiden</a>
  <a href="/doc">dokument</a>
  <a href="/about">ca</a>
</c-nav-menu>
```

## illustrere

Når komponenten er initialisert, hvis det ikke er noen `c-side` i det underordnede elementet, vil mobilmenyknappen og `c-side` sidefeltet bli opprettet automatisk, og alle `a`-taggene vil bli flyttet inn i sidefeltet.

## stilklasse

- **A**: Den aktiverte lenken vil automatisk legge til klassenavnet `A`