# Kopieer tekst naar klembord

Klik op de knop om de tekst van het opgegeven element te kopiëren.

- Ondersteunt het gebruik van rel om de te kopiëren elementselector op te geven
- Als dit niet is opgegeven, wordt het vorige zusterelement standaard gekopieerd.
- Genest Pophover-promptvenster
- Internationale ondersteuning

## Gebruik de demo

```html
<!-- Kopieer specifieke elementen -->
<span id="target">Hier is de tekst die moet worden gekopieerd</span>
<c-copy rel="#target"></c-copy>

<!-- Voorafgaande elementen worden standaard gekopieerd -->
<span>de tekst van het vorige element</span>
<c-copy></c-copy>
```

## parameter

| eigendom | illustreren |
| --- | --- |
| rel | Kopieer de selector van het doelelement. Optioneel |

## status stijl

- `c-pophover.Done`: promptvakstijl na succesvol kopiëren