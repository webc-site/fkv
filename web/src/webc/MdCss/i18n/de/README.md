# Für das Leseerlebnis optimierte Typografiestile

- **Grundlegender Schriftsatz**: Anpassung an Titel, Absätze, Fettschrift, Kursivschrift, Durchstreichung, Inline-Code, Bilder und Trennlinien.
- **Link-Animation**: Beim Bewegen des Mauszeigers wird eine Unterstreichungsanimation angezeigt.
- **Projektliste**: Ungeordnete Listen verwenden Aufzählungszeichen, geordnete Listen verwenden abgerundete Hintergrundseriennummern und unterstützen die Verschachtelung auf mehreren Ebenen.
- **Aufgabenliste**: Stellt den Kontrollkästchenstil bereit und unterstützt den Status „Aufgaben“ und „Erledigt“.
- **Warnhinweise**: Unterstützt fünf Arten von Warnfeldern: Hinweis, Tipp, Wichtig, Warnung und Achtung.
- **Tabellen und Code**: Bietet Hover-Hervorhebungen für Tabellenzeilen sowie Codeblöcke mit Schatten und Verlaufshintergründen.

## Nutzen Sie die Demo

```html
<div class="Md">
  <h1>Titel</h1>
  <p>Absatztext und <a href="#">Link</a></p>
  <hr>
  <h2>Titel der zweiten Ebene</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Aufgabenelement</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Inhalt notieren</p>
  </blockquote>
</div>
```

## Stilklasse

Stile werden auf HTML-Elemente unter dem `.Md`-Container angewendet.

### Alarmbox-Stil

Fügen Sie den folgenden Klassennamen zum Element `blockquote` mit dem Klassennamen `.q` hinzu, um das Warnfeld umzuschalten:

- `note`: Notizen
- `tip`: Tipp
- `important`: Wichtig
- `warning`: Warnung
- `caution`: Hinweis