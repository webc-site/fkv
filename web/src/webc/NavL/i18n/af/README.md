# Skakel outomaties navigasiekieslys en terugkeer-knoppie

- Wikkel kinderelemente outomaties om navigasiekieslys en terugstuurknoppiestrukture te genereer
- Spoor roetegeskiedenis outomaties na en bereken die vorige terugkeerpad
- Vertoon die navigasiekieslys wanneer die roete by die kieslysitem pas, anders vertoon die terugknoppie

## Gebruik die demo

```html
<c-nav-l>
  <a href="/">voorblad</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## styl klas

- `B`: aktiveer die terug-knoppie-toestand. Wys die terugknoppie en versteek die navigasiekieslys. Outomaties oorgeskakel binne die komponent gebaseer op roetering.