# Kopiatu testua arbelean

Egin klik botoian zehaztutako elementuaren testua kopiatzeko.

- rel erabiltzea onartzen du kopiatu beharreko elementu-hautatzailea zehazteko
- Zehazten ez denean, aurreko anai-arreba elementua lehenespenez kopiatzen da.
- Pophover habiaratua gonbita-koadroa
- Nazioarteko laguntza

## Erabili demoa

```html
<!-- Elementu zehatzak kopiatu -->
<span id="target">Hona hemen kopiatu beharreko testua</span>
<c-copy rel="#target"></c-copy>

<!-- Lehenespenez, aurreko elementuak kopiatzen dira -->
<span>aurreko elementuaren testua</span>
<c-copy></c-copy>
```

## parametroa

| jabetza | ilustratu |
| --- | --- |
| rel | Kopiatu xede-elementuaren hautatzailea. Aukerakoa |

## egoera estiloa

- `c-pophover.Done`: gonbita-koadroaren estiloa kopiatu ondoren