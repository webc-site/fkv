# Dewislen llywio wedi'i haddasu i wahanol feintiau sgrin

- Bwrdd Gwaith: Mae dolenni'n cael eu harddangos mewn teils llorweddol
- Fersiwn symudol: Cwympwch i'r botwm dewislen, cliciwch i lithro allan y drôr bar ochr
- Cychwyn llwybr: cymharwch y llwybr presennol a'r gwerth hash yn awtomatig, ac ychwanegwch enw dosbarth statws actifadu i'r ddolen gyfatebol `A`
- Cau'n awtomatig: Pan fydd y derfynell symudol yn clicio ar ddolen neu gefndir allanol y bar ochr, mae'r bar ochr yn cau'n awtomatig.

## defnydd

```html
<c-nav-menu>
  <a href="/">tudalen flaen</a>
  <a href="/doc">dogfen</a>
  <a href="/about">am</a>
</c-nav-menu>
```

## darlunio

Pan ddechreuir y gydran, os nad oes `c-side` yn yr elfen plentyn, bydd y botwm dewislen symudol a `c-side` bar ochr yn cael eu creu'n awtomatig, a bydd yr holl dagiau `a` yn cael eu symud i'r bar ochr.

## dosbarth arddull

- **A**: Bydd y ddolen actifadu yn ychwanegu enw'r dosbarth `A` yn awtomatig