# Umschaltbare Registerkarten

- Der ausgewählte Status des Etiketts wird nahtlos in den darunter liegenden Inhaltsbereich integriert.
- Strukturelle Spezifikationen: Die untergeordneten Elemente sind so festgelegt, dass sie `nav > a` als Beschriftungsnavigation verwenden, und das direkte untergeordnete Element `b` wird als Inhaltsbereich verwendet.
- Verknüpfen Sie das Tag `a[value]` automatisch mit dem Inhaltsknoten `b[slot]`.
- Unterstützt das automatische Beibehalten der aktuell aktiven Seite auf `localStorage` über das Attribut `key`.
- Unterstützt die Angabe der aktivierten Registerkartenseite über das Attribut `value` (wenn kein Cache und `value` vorhanden ist, wird standardmäßig das Tag mit `class="A"` oder das erste Tag ausgewählt).
- Leichte Light-DOM-Implementierung, keine Behinderung durch Shadow-DOM, hohe Freiheit bei der Stilanpassung.

## Nutzen Sie die Demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">Apfel</a>
    <a value="banana">Banane</a>
    <a value="orange">Mandarine</a>
  </nav>
  <b slot="apple">Apfel ist eine nahrhafte Frucht</b>
  <b slot="banana">Banane ist eine tropische Frucht</b>
  <b slot="orange">Orangen sind reich an Vitamin C</b>
</c-tab>
```

## Eigentum

- `key`: Schlüsselname, der für die LocalStorage-Persistenz verwendet wird
- `value`: aktuell ausgewählter Wert

## Benutzerdefinierte Ereignisse

- `change`: Wird beim Wechseln der Registerkarte ausgelöst. `e.value` ist der `value`-Wert des aktuell aktivierten Tags.