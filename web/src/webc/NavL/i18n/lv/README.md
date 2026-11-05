# Automātiski pārslēdziet navigācijas izvēlni un atgriešanas pogu

- Automātiski ietiniet pakārtotos elementus, lai ģenerētu navigācijas izvēlni un atgriešanas pogu struktūras
- Automātiski izsekojiet maršrutēšanas vēsturi un aprēķiniet iepriekšējo atgriešanās ceļu
- Parādiet navigācijas izvēlni, kad maršruts atbilst izvēlnes vienumam, pretējā gadījumā parādiet pogu Atpakaļ

## Izmantojiet demonstrāciju

```html
<c-nav-l>
  <a href="/">sākuma lapa</a>
  <a href="/doc">dokumentu</a>
</c-nav-l>
```

## stila klase

- `B`: aktivizē atgriešanas pogas stāvokli. Parādiet pogu Atpakaļ un paslēpiet navigācijas izvēlni. Automātiski pārslēgts komponentā, pamatojoties uz maršrutēšanu.