# Registerkartenseite mit Dropdown-Auswahl

- Unterstützt das Einbetten eines Dropdown-Auswahlfelds mit adaptiver Breite (`<c-select>`) in die Beschriftung, das nur angezeigt wird, wenn die aktuelle Registerkarte aktiviert ist.
- Strukturelle Spezifikation: Das untergeordnete Element verwendet `nav > a` als Beschriftungsnavigation und das direkte untergeordnete Element `b` dient als Inhaltsbereich.
- Verknüpft das Tag `a[value]`, `a` automatisch mit `c-select` (basierend auf dem Wertattribut der Gruppe und dem ausgewählten Optionswert) und dem Inhaltsknoten `b[slot]`.
- Unterstützt die automatische Beibehaltung des aktuell aktivierten Elements auf `localStorage` über das Attribut `key`.
- Unterstützt das automatische Speichern und Wiederherstellen historischer Unterauswahlen für jede Gruppe in `localStorage` mithilfe von `key + '/' + tab`.
- Unterstützt die Angabe der aktiven Registerkarte über das Attribut `value`.

## Nutzen Sie die Demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>Sprache</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>System</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript ist eine Skriptsprache</b>
    <b slot="rs">Rust ist eine Programmiersprache auf Systemebene</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS ist ein Unix-basiertes grafisches Betriebssystem</b>
    <b slot="win">Windows ist ein von Microsoft entwickeltes Betriebssystem</b>
  </b>
</c-tab-select>
```

## Eigentum

- `key`: Schlüsselname, der für die LocalStorage-Persistenz verwendet wird
- `value`: aktuell ausgewählter Wert

## Benutzerdefinierte Ereignisse

- `change`: Wird beim Wechseln der Registerkarten ausgelöst. `e.value` ist der `value`-Wert der aktuell aktivierten Option.