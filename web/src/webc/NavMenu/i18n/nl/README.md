# Navigatiemenu aangepast aan verschillende schermformaten

- Bureaublad: koppelingen worden weergegeven in horizontale tegels
- Mobiele versie: samenvouwen in de menuknop, klik om de zijbalklade uit te schuiven
- Route-activering: vergelijk automatisch het huidige pad en de hash-waarde, en voeg de klassenaam van de activeringsstatus toe aan de overeenkomende link `A`
- Automatisch sluiten: Wanneer de mobiele terminal op een link of de externe achtergrond van de zijbalk klikt, wordt de zijbalk automatisch gesloten.

## gebruik

```html
<c-nav-menu>
  <a href="/">voorpagina</a>
  <a href="/doc">document</a>
  <a href="/about">over</a>
</c-nav-menu>
```

## illustreren

Wanneer de component wordt geïnitialiseerd en er geen `c-side` in het onderliggende element aanwezig is, worden de mobiele menuknop en de `c-side` zijbalk automatisch gemaakt, en worden alle `a` tags naar de zijbalk verplaatst.

## stijl klasse

- **A**: De geactiveerde link voegt automatisch de klassenaam `A` toe