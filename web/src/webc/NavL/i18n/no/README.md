# Bytt automatisk navigasjonsmeny og returknapp

- Pakk automatisk inn underordnede elementer for å generere navigasjonsmeny og returknappstrukturer
- Spor automatisk rutinghistorikk og beregn forrige returbane
- Vis navigasjonsmenyen når ruten samsvarer med menyelementet, ellers vis tilbakeknappen

## Bruk demoen

```html
<c-nav-l>
  <a href="/">forsiden</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## stilklasse

- `B`: aktiverer tilstanden for returknappen. Vis tilbake-knappen og skjul navigasjonsmenyen. Byttes automatisk i komponenten basert på ruting.