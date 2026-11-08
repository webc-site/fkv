# Kopieer teks na knipbord

Klik op die knoppie om die teks van die gespesifiseerde element te kopieer.

- Ondersteun die gebruik van rel om die elementkieser te spesifiseer wat gekopieer moet word
- Wanneer dit nie gespesifiseer word nie, word die vorige broer of suster by verstek gekopieer.
- Geneste Pophover-boodskapboks
- Internasionale ondersteuning

## Gebruik die demo

```html
<!-- Kopieer spesifieke elemente -->
<span id="target">Hier is die teks wat gekopieer moet word</span>
<c-copy rel="#target"></c-copy>

<!-- Voorafgaande elemente word by verstek gekopieer -->
<span>die teks van die vorige element</span>
<c-copy></c-copy>
```

## parameter

| eiendom | illustreer |
| --- | --- |
| rel | Kopieer die kieser van die teikenelement. Opsioneel |

## status styl

- `c-pophover.Done`: promptboksstyl na suksesvolle kopiëring