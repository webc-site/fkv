# Documentație și cod sursă pentru randarea componentelor sau a funcțiilor instrumentului

- Aspect receptiv pe două coloane, cu documentele afișate în stânga și codul sursă afișat în dreapta
- Documentație și codul sursă acceptă butoanele de comandă rapidă Editare și Cod pentru a trece la adresa sursă.
- Titlul și introducerea sunt afișate în partea de sus, acceptând traducerea automată i18n.

## Utilizați demonstrația

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametrii proprietății

- `src`: adresa URL a codului sursă.
- `href`: adresa URL a reducerii documentului, acceptă utilizarea `{code}` substituent pentru a se adapta la limba curentă.