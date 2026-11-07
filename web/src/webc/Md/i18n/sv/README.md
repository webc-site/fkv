# Analysera och återge Markdown-text på ett säkert sätt till HTML.

- **Säkerhetssanering**: Försvara dig mot XSS-attacker med inbyggda `setHTML` API eller `DOMParser` rengöringsmedel
- **Innehållsextraktion**: Läs automatiskt textinnehållet i standardplatsen under initiering och rendera det
- **Inbyggd typsättning**: förinställd lista, tabell, kodblock, citat och varningsruta stilar

## Använd demon

### statisk rendering

```html
<c-md>
# titel
Det här är en del av Markdown-innehåll
</c-md>
```

### dynamiskt uppdrag

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Ny titel\nNytt innehåll";
</script>
```

## Gränssnittsparametrar

Skicka in Markdown-text direkt i taggen (som standardplats), eller uppdatera dynamiskt via `value`.

## Stilbeskrivning

Stilklassen `.Md` läggs till automatiskt när komponenten initieras. Stöder följande markdown-layout för utökad varningsruta (utlöst av referenssyntax):

- `[!NOTE]`：Promptinformation
- `[!TIP]`: Förslag/tips
- `[!IMPORTANT]`: Viktig uppmärksamhet
- `[!WARNING]`: Riskvarning
- `[!CAUTION]`: Varning för extremt hög risk

## säkerhetsmekanism

När du renderar, anrop först det inbyggda `setHTML` API:et för att säkert injicera HTML. Om webbläsaren inte stöder det kommer den automatiskt att nedgraderas till ett rekursivt rengöringsmedel baserat på `DOMParser` och implementera följande säkerhetspolicy:
- Ta bort osäkra taggar (t.ex. `script`, `iframe` osv.)
- Rensa `on*` händelseattribut
- Försvara dig mot XSS-attacker genom att filtrera attribut som börjar med farliga protokoll som `javascript:`, `vbscript:` och `data:` (förutom `data:image/`).