# Dropdown-Auswahl, Breite passt sich dem aktuellen Artikel an

Die Breite von nativem `select` wird standardmäßig durch den längsten Optionstext bestimmt. Beim Umschalten von kurzen Optionen entsteht zu viel Leerraum, was sich auf das Erscheinungsbild auswirkt.

Diese Komponente deckt transparent das native `select` über dem `b`-Tag ab und ermöglicht so eine genaue Anpassung der Breite an die Länge des aktuell ausgewählten Textes.

- Behalten Sie native `select`-Dropdown-Menüstile und Barrierefreiheitsunterstützung auf allen Terminals bei
- Unterstützt native interaktive Vorgänge wie die vollständige Tab-Navigation auf der Tastatur und das Wechseln der Richtungstasten.
- Passen Sie die Breite an die aktuelle Option an, um überschüssigen Leerraum zu vermeiden

## Nutzen Sie die Demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```