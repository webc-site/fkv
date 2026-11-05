# Pantaila tamaina desberdinetara egokitutako nabigazio menua

- Mahaigaina: estekak lauza horizontaletan bistaratzen dira
- Mugikorrentzako bertsioa: tolestu menuko botoian, egin klik alboko barraren tiraderatik ateratzeko
- Ibilbidearen aktibazioa: automatikoki alderatu uneko bidea eta hash balioa, eta gehitu aktibazio-egoera klasearen izena bat datorren estekan `A`
- Itxi automatikoki: terminal mugikorrak esteka batean edo alboko barraren kanpoko atzeko planoan klik egiten duenean, alboko barra automatikoki ixten da.

## erabili

```html
<c-nav-menu>
  <a href="/">portada</a>
  <a href="/doc">dokumentua</a>
  <a href="/about">buruz</a>
</c-nav-menu>
```

## ilustratu

Osagaia hasieratzen denean, haurraren elementuan `c-side` ez badago, mugikorreko menuaren botoia eta `c-side` alboko barra automatikoki sortuko dira, eta `a` etiketa guztiak alboko barrara eramango dira.

## estilo klasea

- **A**: Aktibatutako estekak `A` klasearen izena automatikoki gehituko du