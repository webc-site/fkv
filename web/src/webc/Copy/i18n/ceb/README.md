# Kopyaha ang teksto sa clipboard

I-klik ang buton aron kopyahon ang teksto sa gipiho nga elemento.

- Nagsuporta sa paggamit sa rel aron mahibal-an ang tigpili sa elemento nga kopyahon
- Kung wala gitino, ang miaging elemento sa igsoon gikopya pinaagi sa default.
- Nested Pophover prompt box
- Internasyonal nga suporta

## Gamita ang demo

```html
<!-- Kopyaha ang piho nga mga elemento -->
<span id="target">Ania ang teksto nga kopyahon</span>
<c-copy rel="#target"></c-copy>

<!-- Ang nag-una nga mga elemento gikopya pinaagi sa default -->
<span>ang teksto sa miaging elemento</span>
<c-copy></c-copy>
```

## parametro

| kabtangan | iilustrar |
| --- | --- |
| rel | Kopyaha ang tigpili sa target nga elemento. Opsyonal |

## estilo sa kahimtang

- `c-pophover.Done`: estilo sa kahon sa pag-aghat human sa malampusong pagkopya