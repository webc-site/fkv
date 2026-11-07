# Parse og gengiv Markdown-tekst sikkert til HTML.

- **Sikkerhedsrensning**: Forsvar mod XSS-angreb ved hjælp af native `setHTML` API eller `DOMParser` rensemidler
- **Indholdsudtrækning**: Læs automatisk tekstindholdet i standardpladsen under initialisering og gengiv det
- **Indbygget typesetting**: forudindstillet liste, tabel, kodeblok, citat og advarselsboksstile

## Brug demoen

### statisk gengivelse

```html
<c-md>
# titel
Dette er et stykke Markdown-indhold
</c-md>
```

### dynamisk opgave

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Ny titel\nNyt indhold";
</script>
```

## Interface parametre

Send Markdown-tekst direkte i tagget (som standardpladsen), eller opdater dynamisk via `value`.

## Stilbeskrivelse

Stilklassen `.Md` tilføjes automatisk, når komponenten initialiseres. Understøtter følgende Markdown udvidede advarselsbokslayout (udløst af referencesyntaks):

- `[!NOTE]`：Spørgsmålsoplysninger
- `[!TIP]`: Forslag/tip
- `[!IMPORTANT]`: Vigtig opmærksomhed
- `[!WARNING]`: Risikoadvarsel
- `[!CAUTION]`: Advarsel om ekstrem høj risiko

## sikkerhedsmekanisme

Ved gengivelse skal du først kalde den oprindelige `setHTML` API for sikkert at indsprøjte HTML. Hvis browseren ikke understøtter det, vil den automatisk nedgradere til en rekursiv renser baseret på `DOMParser` og implementere følgende sikkerhedspolitik:
- Fjern ikke-sikre tags (f.eks. `script`, `iframe` osv.)
- Ryd `on*` hændelsesattributter
- Forsvar mod XSS-angreb ved at filtrere attributter, der starter med farlige protokoller såsom `javascript:`, `vbscript:` og `data:` (undtagen `data:image/`).