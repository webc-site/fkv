# Menú de navegació adaptat a diferents mides de pantalla

- Escriptori: els enllaços es mostren en rajoles horitzontals
- Versió mòbil: replega al botó de menú, fes clic per lliscar el calaix de la barra lateral
- Activació de la ruta: compara automàticament el camí i el valor hash actuals i afegeix el nom de la classe d'estat d'activació a l'enllaç coincident `A`
- Tancament automàtic: quan el terminal mòbil fa clic a un enllaç o al fons extern de la barra lateral, la barra lateral es tanca automàticament.

## utilitzar

```html
<c-nav-menu>
  <a href="/">portada</a>
  <a href="/doc">document</a>
  <a href="/about">sobre</a>
</c-nav-menu>
```

## il·lustrar

Quan s'inicialitzi el component, si no hi ha `c-side` a l'element secundari, es crearan automàticament el botó del menú mòbil i la barra lateral `c-side` i totes les etiquetes `a` es mouran a la barra lateral.

## classe d'estil

- **A**: l'enllaç activat afegirà automàticament el nom de la classe `A`