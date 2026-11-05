# Kopyahin ang teksto sa clipboard

I-click ang pindutan upang kopyahin ang teksto ng tinukoy na elemento.

- Sinusuportahan ang paggamit ng rel upang tukuyin ang tagapili ng elemento na kokopyahin
- Kapag hindi tinukoy, ang dating elemento ng kapatid ay kinokopya bilang default.
- Nested Pophover prompt box
- Internasyonal na suporta

## Gamitin ang demo

```html
<!-- Kopyahin ang mga partikular na elemento -->
<span id="target">Narito ang teksto na dapat kopyahin</span>
<c-copy rel="#target"></c-copy>

<!-- Ang mga naunang elemento ay kinopya bilang default -->
<span>ang teksto ng nakaraang elemento</span>
<c-copy></c-copy>
```

## parameter

| ari-arian | ilarawan |
| --- | --- |
| rel | Kopyahin ang tagapili ng target na elemento. Opsyonal |

## istilo ng katayuan

- `c-pophover.Done`: istilo ng prompt na kahon pagkatapos ng matagumpay na pagkopya