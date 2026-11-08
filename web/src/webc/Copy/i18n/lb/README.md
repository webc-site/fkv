# Kopéiert Text op Clipboard

Klickt op de Knäppchen fir den Text vum spezifizéierte Element ze kopéieren.

- Ënnerstëtzt benotzt rel fir den Elementselektor ze spezifizéieren fir ze kopéieren
- Wann net uginn, gëtt dat viregt Geschwësterelement par défaut kopéiert.
- Nested Pophover Prompt Box
- International Ënnerstëtzung

## Benotzt d'Demo

```html
<!-- Kopie spezifesch Elementer -->
<span id="target">Hei ass den Text fir ze kopéieren</span>
<c-copy rel="#target"></c-copy>

<!-- Virun Elementer ginn als Standard kopéiert -->
<span>den Text vum virdrun Element</span>
<c-copy></c-copy>
```

## parameter

| Propriétéit | illustréieren |
| --- | --- |
| rel | Kopéiert de Selektor vum Zilelement. Fakultativ |

## Status Stil

- `c-pophover.Done`: Prompt Box Style no erfollegräicher Kopie