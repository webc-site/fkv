# Ikkopja t-test fil-clipboard

Ikklikkja l-buttuna biex tikkopja t-test tal-element speċifikat.

- Jappoġġja l-użu ta 'rel biex jispeċifika s-selettur tal-element li għandu jiġi kkupjat
- Meta ma jkunx speċifikat, l-element tal-aħwa preċedenti jiġi kkupjat awtomatikament.
- Kaxxa fil-pront nested Pophover
- Appoġġ internazzjonali

## Uża d-demo

```html
<!-- Ikkopja elementi speċifiċi -->
<span id="target">Hawn hu t-test li jrid jiġi kkupjat</span>
<c-copy rel="#target"></c-copy>

<!-- L-elementi preċedenti huma kkupjati awtomatikament -->
<span>it-test tal-element preċedenti</span>
<c-copy></c-copy>
```

## parametru

| proprjetà | juru |
| --- | --- |
| rel | Ikkopja s-selettur tal-element fil-mira. Fakultattiv |

## stil ta 'status

- `c-pophover.Done`: stil ta 'kaxxa fil-pront wara ikkupjar b'suċċess