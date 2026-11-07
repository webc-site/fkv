# Blog-Beitragskomponente

- **Automatische Jahresgruppierung**: Jahre automatisch nach Artikeldatum klassifizieren und in umgekehrter Reihenfolge nach Jahr anordnen
- **Vereinfachtes Datumsformat**: Entfernen Sie bei der Anzeige automatisch das Jahrespräfix und zeigen Sie nur den Monat und den Tag an
- **Markdown-Zusammenfassung und Text**: Integrierte Listenzusammenfassungsvorschau und vollständige Textwiedergabe einzelner Artikel
- **Dreidimensionaler Titel mit Farbverlauf**: Integrieren Sie `H1Txt`, um die Überschrift und das Veröffentlichungsdatum anzuzeigen

## Nutzen Sie die Demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  „2026/publish“: [„title“, „abstract“, „2026-07-27“]
};

// Wechseln, um den angegebenen Artikeltext zu laden (übergeben Sie eine leere Zeichenfolge, um zurück zur Liste zu wechseln)
blog.load("2026/publish");
</script>
```

## Schnittstellenparameter

### Eigenschaften und Methoden

- `blog.prefix`: Abrufen oder Festlegen des Artikel-Link-Präfixes (z. B. `"/blog/"`)
- `blog.mdUrl`: Rufen Sie die Generierungsfunktion für Markdown-Artikeladressen `(key, lang) => url` ab oder legen Sie sie fest
- `blog._`: Abrufen oder Festlegen des Artikellisten-Metadatenwörterbuchs `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Wechseln Sie, um den Artikelinhalt des angegebenen Schlüssels zu laden. Übergeben Sie eine leere Zeichenfolge `""`, um zurück zur Blogliste zu wechseln