# Anzeige des Lade- oder Bearbeitungsstatus

- Bringen Sie den Cursor in den Wartezustand
- Zeigt eine Wellenanimation an, um den Verarbeitungsstatus anzuzeigen
- Wenden Sie eine Unschärfemaske auf ein Formular an, um doppelte Übermittlungen zu verhindern

## Nutzen Sie die Demo

```html
<!-- unabhängige Ladeanzeige -->
<div class="Ing"></div>

<!-- Formular mit Maske laden -->
<form class="Ing">
  <input type="text" placeholder="Eingabefeld">
  <button type="submit">einreichen</button>
</form>
```

## Stilklasse

### `.Ing`
- Zeigerstatus ist `wait`.
- Das Pseudoelement `::before` rendert eine Wellenanimation.
- Bei Verwendung für das `form`-Element rendert das Pseudoelement `::after` eine 1-Pixel-Unschärfemaske, wodurch Interaktion und Textauswahl deaktiviert werden.