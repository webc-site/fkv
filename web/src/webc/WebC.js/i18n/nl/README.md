# Documentatie en broncode voor het weergeven van componenten of toolfuncties

- Responsieve lay-out met twee kolommen, waarbij documenten aan de linkerkant worden weergegeven en de broncode aan de rechterkant
- Ondersteuning voor documentatie en broncode Sneltoetsen Bewerken en Coderen om naar het bronadres te springen.
- De titel en inleiding worden bovenaan weergegeven en ondersteunen de automatische vertaling van i18n.

## Gebruik de demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Eigenschapsparameters

- `src`: URL-adres van de broncode.
- `href`: URL-adres van documentmarkering, ondersteunt het gebruik van `{code}` tijdelijke aanduiding om aan te passen aan de huidige taal.