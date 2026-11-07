# Verzeichnisnavigationskomponente

- **Reaktionsfähige Schublade**: Seitenleiste im Breitbildmodus anzeigen, automatisch im Schmalbildschirm ausblenden und mit der `c-side`-Schublade und der Titelauslöseschaltfläche zusammenarbeiten
- **Mehrsprachiger Treiber**: Unterstützt Objektwörterbuch oder mehrsprachige Funktionen, reagiert automatisch auf Sprachwechsel
- **Automatisches Unterverzeichnis**: Analysieren Sie die Titel `h1`~`h6` im Zielcontainer automatisch, um ein sekundäres Verzeichnis zu generieren und die Hervorhebung des Bildlaufs im Hash-Ansichtsfenster zu überwachen.
- **Asynchrones Laden**: Klicken Sie auf ein Verzeichniselement, um Markdown-Text automatisch asynchron abzurufen und in den Inhaltsbereich einzufügen, wobei eine `Ing`-Ladeanimation angezeigt wird

## Nutzen Sie die Demo

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  „use“: „Komponente verwenden“
});
</script>
```

## Schnittstellenparameter

### Eigenschaften und Methoden

- `toc.prefix`: Rufen Sie die Routing-Präfixzeichenfolge ab oder legen Sie sie fest (z. B. `"doc"`, der abschließende Schrägstrich ist optional). Der Schlüssel `README` wird automatisch einem leeren Pfad (d. h. dem Präfix selbst) zugeordnet.
- `toc.li`: Abrufen oder Festlegen der mehrsprachigen Verzeichnisgenerierungsfunktion `(lang) => ({ key: title })`
- `toc.mdUrl`: Rufen Sie die Markdown-URL-Generierungsfunktion `(key, lang) => url` ab oder legen Sie sie fest
- `toc.target`: Rufen Sie den zugehörigen Ziel-Rendering-Knoten ab oder legen Sie ihn fest (z. B. `c-md`). Wenn nicht angegeben, wird `c-md` / `main` / `article` automatisch gefunden
- `toc.load(key)`: Manuelles Auslösen des Ladens von Dokumenten und der Verzeichniserweiterung entsprechend dem Schlüssel
- `toc.sync()`: Analysieren Sie den Titel im aktuellen Zielknoten manuell erneut, synchronisieren Sie das sekundäre Verzeichnis und markieren Sie den Status