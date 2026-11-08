# Kopiera text till urklipp

Klicka på knappen för att kopiera texten för det angivna elementet.

- Stöder användning av rel för att specificera elementväljaren som ska kopieras
- När det inte anges kopieras det föregående syskonelementet som standard.
- Kapslad Pophover-promptlåda
- Internationellt stöd

## Använd demon

```html
<!-- Kopiera specifika element -->
<span id="target">Här är texten som ska kopieras</span>
<c-copy rel="#target"></c-copy>

<!-- Föregående element kopieras som standard -->
<span>texten i föregående element</span>
<c-copy></c-copy>
```

## parameter

| egendom | illustrera |
| --- | --- |
| rel | Kopiera väljaren för målelementet. Frivillig |

## statusstil

- `c-pophover.Done`: stil för promptruta efter lyckad kopiering