# Text in die Zwischenablage kopieren

Klicken Sie auf die Schaltfläche, um den Text des angegebenen Elements zu kopieren.

- Unterstützt die Verwendung von rel zur Angabe des zu kopierenden Elementselektors
- Wenn nichts angegeben wird, wird das vorherige Geschwisterelement standardmäßig kopiert.
- Verschachteltes Pophover-Eingabeaufforderungsfeld
- Internationale Unterstützung

## Nutzen Sie die Demo

```html
<!-- Kopieren Sie bestimmte Elemente -->
<span id="target">Hier ist der zu kopierende Text</span>
<c-copy rel="#target"></c-copy>

<!-- Vorhergehende Elemente werden standardmäßig kopiert -->
<span>der Text des vorherigen Elements</span>
<c-copy></c-copy>
```

## Parameter

| Eigentum | veranschaulichen |
| --- | --- |
| rel | Kopieren Sie den Selektor des Zielelements. Optional |

## Statusstil

- `c-pophover.Done`: Stil der Eingabeaufforderungsbox nach erfolgreichem Kopieren