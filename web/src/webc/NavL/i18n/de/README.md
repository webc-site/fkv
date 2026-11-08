# Navigationsmenü und Zurück-Taste automatisch wechseln

- Untergeordnete Elemente automatisch umschließen, um Navigationsmenü- und Zurück-Schaltflächenstrukturen zu generieren
- Verfolgen Sie automatisch den Routing-Verlauf und berechnen Sie den vorherigen Rückweg
- Zeigen Sie das Navigationsmenü an, wenn die Route mit dem Menüpunkt übereinstimmt, andernfalls zeigen Sie die Zurück-Taste an

## Nutzen Sie die Demo

```html
<c-nav-l>
  <a href="/">Titelseite</a>
  <a href="/doc">dokumentieren</a>
</c-nav-l>
```

## Stilklasse

- `B`: Aktiviert den Zustand der Zurück-Schaltfläche. Zeigen Sie die Zurück-Schaltfläche an und blenden Sie das Navigationsmenü aus. Wird innerhalb der Komponente basierend auf dem Routing automatisch umgeschaltet.