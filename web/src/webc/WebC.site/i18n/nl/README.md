# Routingportal met documentatie voor componentbibliotheek en demosite

- **Indeling op volledig scherm**: Absolute positionering vult het kijkvenster, met ingebouwd scrollen en navigatie.
- **Dynamische routering**: parseer URL-paden en laad componentdemo's of startpagina's dynamisch.
- **Meertalig**: Reageer en update de navigatiebalk en paginakopie in realtime.
- **Statuswisseling**: behoud automatisch de stijlklassen van het laden en de startpagina.

## Gebruik de demo

```html
<c-webc-site></c-webc-site>
```

## stijl klasse

De component zal de volgende klassenamen binnen zichzelf omschakelen:

- `Ing`: laden van routes of parseren van taalpakketten.
- `A`: momenteel op de startpagina.