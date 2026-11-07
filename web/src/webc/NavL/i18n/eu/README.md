# Aldatu nabigazio menua eta itzultzeko botoia automatikoki

- Bilatu automatikoki elementu seme-alabak nabigazio menua sortzeko eta botoien egiturak itzultzeko
- Jarraitu automatikoki bideratze-historia eta kalkulatu aurreko itzulerako bidea
- Bistaratu nabigazio-menua ibilbidea menuko elementuarekin bat datorrenean, bestela, bistaratu atzera botoia

## Erabili demoa

```html
<c-nav-l>
  <a href="/">portada</a>
  <a href="/doc">dokumentua</a>
</c-nav-l>
```

## estilo klasea

- `B`: itzultzeko botoiaren egoera aktibatzen du. Erakutsi atzera botoia eta ezkutatu nabigazio menua. Osagaiaren barruan automatikoki aldatu da bideratzearen arabera.