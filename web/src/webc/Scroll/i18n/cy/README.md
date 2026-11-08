# bar sgrolio rhithwir

- **Sgrolio dwy ffordd**: Yn darparu sgrolio fertigol (`c-vs`) a llorweddol (`c-hs`).
- **Ymateb maint**: Monitro newidiadau cynnwys a maint ardal, diweddaru maint a lleoliad y bar sgrolio yn awtomatig, a chuddio'n awtomatig pan nad oes gorlif.
- **Cymorth rhyngweithiol**: Yn cefnogi sgrolio olwynion, llusgo llithrydd a lleoli clic trac.
- **Cuddio Statws**: Dangoswch ac ehangwch y llithrydd wrth hofran neu sgrolio, a'i guddio'n awtomatig pan fydd yn llonydd.

## sgrôl fertigol

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## sgrôl llorweddol

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## rhannau arddull

Addaswch arddull elfennau mewnol Shadow DOM trwy `::part()`:

- `::part(scroll)`: Sgroliwch yr ardal gynnwys.
- `::part(bar)`: trac bar sgrolio.
- `::part(si)`: llithrydd bar sgrolio.
- `::part(i)`: Haen lapio fewnol.
- `::part(drag)`: Y trac bar sgrolio yn y cyflwr llusgo.

## dosbarth arddull

- `.drag`: Wedi'i gymhwyso i'r elfen `body`, gan nodi ei fod mewn cyflwr llusgo-sgrolio.

## Priodweddau personol

- `--si-bg`: Lliw cefndir y llithrydd.
- `--si-anim`: Animeiddiad llithrydd.
- `--cursorScrollhSvg`: Cyrchwr y llygoden pan fydd y bar sgrolio llorweddol yn hofran.
- `--cursorScrollvSvg`: Cyrchwr y llygoden pan fydd y bar sgrolio fertigol yn hofran.