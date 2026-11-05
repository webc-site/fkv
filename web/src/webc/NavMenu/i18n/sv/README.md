# Navigationsmeny anpassad till olika skärmstorlekar

- Desktop: Länkar visas i horisontella brickor
- Mobilversion: Tryck ihop i menyknappen, klicka för att dra ut sidofältslådan
- Ruttaktivering: jämför automatiskt aktuell sökväg och hashvärde och lägg till aktiveringsstatusklassnamnet till den matchande länken `A`
- Stäng automatiskt: När mobilterminalen klickar på en länk eller sidofältets externa bakgrund stängs sidofältet automatiskt.

## använda

```html
<c-nav-menu>
  <a href="/">förstasidan</a>
  <a href="/doc">dokumentera</a>
  <a href="/about">om</a>
</c-nav-menu>
```

## illustrera

När komponenten är initierad, om det inte finns någon `c-side` i det underordnade elementet, skapas den mobila menyknappen och `c-side` sidofältet automatiskt, och alla `a` taggar kommer att flyttas till sidofältet.

## stilklass

- **A**: Den aktiverade länken lägger automatiskt till klassnamnet `A`