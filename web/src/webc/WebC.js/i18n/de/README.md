# Dokumentation und Quellcode zum Rendern von Komponenten oder Toolfunktionen

- Zweispaltiges responsives Layout, wobei links Dokumente und rechts Quellcode angezeigt werden
- Dokumentation und Quellcode unterstützen die Tastenkombinationen „Bearbeiten“ und „Code“, um zur Quelladresse zu springen.
- Der Titel und die Einleitung werden oben angezeigt und unterstützen die automatische Übersetzung von i18n.

## Nutzen Sie die Demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Eigenschaftsparameter

- `src`: URL-Adresse des Quellcodes.
- `href`: URL-Adresse des Dokument-Markdowns, unterstützt die Verwendung des Platzhalters `{code}` zur Anpassung an die aktuelle Sprache.