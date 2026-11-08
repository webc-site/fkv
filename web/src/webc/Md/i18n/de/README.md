# Analysieren und rendern Sie Markdown-Text sicher in HTML.

- **Sicherheitsbereinigung**: Schützen Sie sich vor XSS-Angriffen mit nativer `setHTML` API oder `DOMParser` Cleanern
- **Inhaltsextraktion**: Liest den Textinhalt des Standard-Slots während der Initialisierung automatisch und rendert ihn
- **Integrierter Schriftsatz**: voreingestellte Listen-, Tabellen-, Codeblock-, Anführungszeichen- und Warnfeldstile

## Nutzen Sie die Demo

### statisches Rendering

```html
<c-md>
# Titel
Dies ist ein Teil des Markdown-Inhalts
</c-md>
```

### dynamische Zuordnung

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Neuer Titel\nNeuer Inhalt";
</script>
```

## Schnittstellenparameter

Übergeben Sie Markdown-Text direkt im Tag (als Standard-Slot) oder aktualisieren Sie ihn dynamisch über `value`.

## Stilbeschreibung

Die Stilklasse `.Md` wird automatisch hinzugefügt, wenn die Komponente initialisiert wird. Unterstützt das folgende erweiterte Warnfeldlayout von Markdown (ausgelöst durch Referenzsyntax):

- `[!NOTE]`：Prompte Informationen
- `[!TIP]`: Vorschläge/Tipps
- `[!IMPORTANT]`: Wichtiger Hinweis
- `[!WARNING]`: Risikowarnung
- `[!CAUTION]`: Warnung vor extrem hohem Risiko

## Sicherheitsmechanismus

Rufen Sie beim Rendern zuerst die native `setHTML`-API auf, um HTML sicher einzufügen. Wenn der Browser dies nicht unterstützt, wird er automatisch auf einen rekursiven Reiniger basierend auf `DOMParser` heruntergestuft und die folgende Sicherheitsrichtlinie implementieren:
- Entfernen Sie nicht sichere Tags (z. B. `script`, `iframe` usw.)
- Löschen Sie `on*` Ereignisattribute
- Schützen Sie sich vor XSS-Angriffen, indem Sie Attribute filtern, die mit gefährlichen Protokollen wie `javascript:`, `vbscript:` und `data:` (außer `data:image/`) beginnen.