# Lülitage automaatselt navigeerimismenüü ja naasmisnupp

- Pakkige alamelemendid automaatselt, et luua navigeerimismenüü ja tagastamisnupu struktuurid
- Jälgige automaatselt marsruudi ajalugu ja arvutage eelmine tagasitee
- Kuvage navigeerimismenüü, kui marsruut vastab menüüelemendile, vastasel juhul kuvage tagasinupp

## Kasutage demo

```html
<c-nav-l>
  <a href="/">esileht</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## stiiliklass

- `B`: aktiveerib naasmisnupu oleku. Näita tagasi nuppu ja peida navigeerimismenüü. Automaatselt lülitub komponendi sees marsruutimise alusel.