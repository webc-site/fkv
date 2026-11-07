# Sécher parséieren a renderen Markdown Text op HTML.

- **Sécherheetssanéierung**: Verteidegt géint XSS Attacke mat native `setHTML` API oder `DOMParser` Botzmëttelen
- ** Inhaltsextraktioun **: Liest automatesch den Textinhalt vum Standardslot wärend der Initialiséierung a rendert et
- ** Built-in Typesetting **: Preset Lëscht, Dësch, Code Block, Zitat a Warnkëschtstiler

## Benotzt d'Demo

### statesch Rendering

```html
<c-md>
# Titel
Dëst ass e Stéck Markdown Inhalt
</c-md>
```

### dynamesch Aufgab

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Neien Titel\nNeien Inhalt";
</script>
```

## Interface Parameteren

Gitt Markdown Text direkt am Tag (als Standard Slot), oder aktualiséieren dynamesch iwwer `value`.

## Style Beschreiwung

D'`.Md` Stil Klass gëtt automatesch bäigefüügt wann de Komponent initialiséiert gëtt. Ënnerstëtzt de folgende Markdown erweiderten Warnkëscht Layout (ausgeléist duerch Referenz Syntax):

- `[!NOTE]`：Informatioun prompt
- `[!TIP]`: Virschléi/Tipps
- `[!IMPORTANT]`: Wichteg Opmierksamkeet
- `[!WARNING]`: Risikowarnung
- `[!CAUTION]`: Extrem héich Risiko Warnung

## Sécherheet Mechanismus

Beim Rendering, rufft déi gebierteg `setHTML` API fir d'éischt fir sécher HTML ze sprëtzen. Wann de Browser et net ënnerstëtzt, gëtt en automatesch op e rekursive Botz erofgradéiert baséiert op `DOMParser` an implementéiert déi folgend Sécherheetspolitik:
- Ewechzehuelen net-sécher Tags (zB `script`, `iframe`, etc.)
- Kloer `on*` Event Attributer
- Verteidegt géint XSS Attacken andeems Dir Attributer filtert mat geféierleche Protokoller wéi `javascript:`, `vbscript:` an `data:` (ausser `data:image/`).