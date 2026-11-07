# Parseer en render Markdown-tekst veilig naar HTML.

- **Beveiligingsopschoning**: verdedig je tegen XSS-aanvallen met behulp van native `setHTML` API of `DOMParser` opschoonprogramma's
- **Inhoudsextractie**: Lees automatisch de tekstinhoud van het standaardslot tijdens initialisatie en geef deze weer
- **Ingebouwde typografie**: vooraf ingestelde lijst-, tabel-, codeblok-, aanhalingstekens- en waarschuwingsvakstijlen

## Gebruik de demo

### statische weergave

```html
<c-md>
# titel
Dit is een stukje Markdown-inhoud
</c-md>
```

### dynamische opdracht

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nieuwe titel\nNieuwe inhoud";
</script>
```

## Interfaceparameters

Geef Markdown-tekst rechtstreeks door in de tag (als standaardruimte), of update dynamisch via `value`.

## Stijlbeschrijving

De stijlklasse `.Md` wordt automatisch toegevoegd wanneer de component wordt geïnitialiseerd. Ondersteunt de volgende uitgebreide Markdown-waarschuwingsvakindeling (geactiveerd door referentiesyntaxis):

- `[!NOTE]`：Promptinformatie
- `[!TIP]`: Suggesties/Tips
- `[!IMPORTANT]`: Belangrijke aandacht
- `[!WARNING]`: Risicowaarschuwing
- `[!CAUTION]`: waarschuwing voor extreem hoog risico

## beveiligingsmechanisme

Roep bij het renderen eerst de native `setHTML` API aan om HTML veilig te injecteren. Als de browser dit niet ondersteunt, zal deze automatisch downgraden naar een recursieve opschoner op basis van `DOMParser` en het volgende beveiligingsbeleid implementeren:
- Verwijder niet-veilige tags (bijvoorbeeld `script`, `iframe`, enz.)
- Wis `on*` gebeurteniskenmerken
- Verdedig je tegen XSS-aanvallen door attributen te filteren die beginnen met gevaarlijke protocollen zoals `javascript:`, `vbscript:` en `data:` (behalve `data:image/`).