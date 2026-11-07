# Copiați textul în clipboard

Faceți clic pe butonul pentru a copia textul elementului specificat.

- Acceptă utilizarea rel pentru a specifica selectorul de element care urmează să fie copiat
- Când nu este specificat, elementul frate anterior este copiat implicit.
- Casetă promptă imbricată Pophover
- Sprijin internațional

## Utilizați demonstrația

```html
<!-- Copiați elemente specifice -->
<span id="target">Iată textul de copiat</span>
<c-copy rel="#target"></c-copy>

<!-- Elementele precedente sunt copiate implicit -->
<span>textul elementului anterior</span>
<c-copy></c-copy>
```

## parametru

| proprietate | ilustra |
| --- | --- |
| rel | Copiați selectorul elementului țintă. Opțional |

## stil de statut

- `c-pophover.Done`: stil de casetă promptă după copierea cu succes