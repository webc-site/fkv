# Zkopírujte text do schránky

Klepnutím na tlačítko zkopírujete text zadaného prvku.

- Podporuje použití rel k určení selektoru prvků, který se má zkopírovat
- Není-li zadán, je ve výchozím nastavení zkopírován předchozí sourozenecký prvek.
- Vnořené pole výzvy Pophover
- Mezinárodní podpora

## Použijte demo

```html
<!-- Zkopírujte konkrétní prvky -->
<span id="target">Zde je text ke zkopírování</span>
<c-copy rel="#target"></c-copy>

<!-- Předchozí prvky se ve výchozím nastavení zkopírují -->
<span>text předchozího prvku</span>
<c-copy></c-copy>
```

## parametr

| vlastnictví | ilustrovat |
| --- | --- |
| rel | Zkopírujte selektor cílového prvku. Volitelný |

## stavový styl

- `c-pophover.Done`: styl pole výzvy po úspěšném zkopírování