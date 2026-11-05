# Navigationsmenü angepasst an verschiedene Bildschirmgrößen

- Desktop: Links werden in horizontalen Kacheln angezeigt
- Mobile Version: Klicken Sie auf die Schaltfläche „In Menü einklappen“ und klicken Sie, um die Seitenleistenschublade herauszuschieben
- Routenaktivierung: Vergleichen Sie automatisch den aktuellen Pfad und den Hash-Wert und fügen Sie den Namen der Aktivierungsstatusklasse zum passenden Link `A` hinzu
- Automatisch schließen: Wenn das mobile Endgerät auf einen Link oder den externen Hintergrund der Seitenleiste klickt, wird die Seitenleiste automatisch geschlossen.

## verwenden

```html
<c-nav-menu>
  <a href="/">Titelseite</a>
  <a href="/doc">dokumentieren</a>
  <a href="/about">um</a>
</c-nav-menu>
```

## veranschaulichen

Wenn bei der Initialisierung der Komponente kein `c-side` im untergeordneten Element vorhanden ist, werden die mobile Menüschaltfläche und die `c-side`-Seitenleiste automatisch erstellt und alle `a`-Tags werden in die Seitenleiste verschoben.

## Stilklasse

- **A**: Der aktivierte Link fügt automatisch den Klassennamen `A` hinzu