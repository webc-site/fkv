# Eingabeaufforderung für schwebende Blasen

Zeigen Sie Hover-Tooltip-Informationen an, wenn Sie mit der Maus darüber fahren oder das Element aktiviert ist.

- Basierend auf einer nativen Webkomponente, leicht und leistungsstark
- Unterstützt adaptive räumliche Positionierung in vier Richtungen (class="top/bottom/left/right")
- Es verwendet einen Hintergrund mit schwarzem Milchglas-Texturverlauf und weißen Zeichen mit exquisiten physischen Kantenkontur-Highlights.
- Die integrierte Positionsanpassungsberechnung passt die Popup-Richtung automatisch an den verbleibenden Platz im Ansichtsfenster an
- Verwenden Sie `slot="pop"`, um schwebende Eingabeaufforderungsinhalte anzuzeigen

## Nutzen Sie die Demo

```html
<!-- Richtung automatisch berechnen (empfohlen, keine Angabe der Klasse erforderlich) -->
<c-pophover>
  <button>automatisch</button>
  <div slot="pop">Dies ist ein Blasentipp</div>
</c-pophover>

<!-- feste Richtung -->
<c-pophover class="left">
  <button>Links</button>
  <div slot="pop">Dies ist ein Blasentipp</div>
</c-pophover>

<c-pophover class="right">
  <button>Rechts</button>
  <div slot="pop">Dies ist ein Blasentipp</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Runter</button>
  <div slot="pop">Dies ist ein Blasentipp</div>
</c-pophover>

<c-pophover class="top">
  <button>Vorgesetzter</button>
  <div slot="pop">Dies ist ein Blasentipp</div>
</c-pophover>
```

## CSS-Variablen

Benutzerdefinierte Stile werden über die folgenden CSS-Variablen unterstützt:

| Variablenname | Standardwert | veranschaulichen |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Der Raum zwischen dem Blasen-Tooltip und dem auslösenden Element |
| `--pophover-margin` | `24px` | Sicherheitsmarge für die Kante des Ansichtsfensters während der adaptiven Positionierungsberechnung |
| `--pophover-bg-top` | `#18181ce0` | Obere Hintergrundfarbe mit Blasenverlauf |
| `--pophover-bg-mid` | `#121215e5` | Die Hintergrundfarbe der Pfeile auf der linken und rechten Seite der Blase |
| `--pophover-bg-bottom` | `#0c0c0feb` | Die Farbe der unteren und oberen und unteren Seitenpfeile des Hintergrunds mit Blasenverlauf |

## Symbol-Tooltip-Stil (`.Ico`)

Für schwebende Eingabeaufforderungsschaltflächen vom reinen Symboltyp (z. B. „Bearbeiten“, „Löschen“ usw.) können Sie einen einheitlichen Symbol- und Hover-Stil anwenden, indem Sie den Klassennamen `.Ico` zum äußeren Container oder zur Komponente selbst hinzufügen.

### Strukturbeispiel

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">bearbeiten</div>
  </c-pophover>
</b>
```

Das grundlegende Layout, die Größe (18 x 18 Pixel) und die Änderungen der Hover-Farbe wurden in `.Ico > c-pophover > a` vereinheitlicht. Entwickler müssen nur die entsprechende Symboldatei separat angeben, z. B.:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```