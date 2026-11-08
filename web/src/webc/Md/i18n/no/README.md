# Parse og gjengi Markdown-tekst på en sikker måte til HTML.

- **Sikkerhetsrensing**: Forsvar mot XSS-angrep ved å bruke native `setHTML` API eller `DOMParser` rengjøringsmidler
- **Innholdsekstraksjon**: Les automatisk tekstinnholdet i standardsporet under initialisering og gjengi det
- **Innebygd typesetting**: forhåndsinnstilt liste, tabell, kodeblokk, sitat og advarselsboksstiler

## Bruk demoen

### statisk gjengivelse

```html
<c-md>
# tittel
Dette er et stykke Markdown-innhold
</c-md>
```

### dynamisk oppdrag

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Ny tittel\nNytt innhold";
</script>
```

## Grensesnittparametere

Send inn Markdown-tekst direkte i taggen (som standardplass), eller oppdater dynamisk via `value`.

## Stilbeskrivelse

Stilklassen `.Md` legges automatisk til når komponenten initialiseres. Støtter følgende Markdown utvidede advarselsboksoppsett (utløst av referansesyntaks):

- `[!NOTE]`：Spørsmålsinformasjon
- `[!TIP]`: Forslag/tips
- `[!IMPORTANT]`: Viktig oppmerksomhet
- `[!WARNING]`: Risikoadvarsel
- `[!CAUTION]`: Ekstremt høyrisikoadvarsel

## sikkerhetsmekanisme

Når du gjengir, ring først den opprinnelige `setHTML` API-en for å injisere HTML på en sikker måte. Hvis nettleseren ikke støtter det, vil den automatisk nedgradere til en rekursiv renser basert på `DOMParser` og implementere følgende sikkerhetspolicy:
- Fjern usikre tagger (f.eks. `script`, `iframe` osv.)
- Slett `on*` hendelsesattributter
- Forsvar mot XSS-angrep ved å filtrere attributter som starter med farlige protokoller som `javascript:`, `vbscript:` og `data:` (unntatt `data:image/`).