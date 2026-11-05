# Vísir um hleðslu eða vinnslustöðu

- Sameinaðu bendilinn í biðstöðu
- Sýnir gára hreyfimynd til að gefa til kynna vinnslustöðu
- Settu óskýra grímu á eyðublað til að koma í veg fyrir tvíteknar sendingar

## Notaðu kynninguna

```html
<!-- óháður hleðsluvísir -->
<div class="Ing"></div>

<!-- Hleðsluform með grímu -->
<form class="Ing">
  <input type="text" placeholder="Inntaksbox">
  <button type="submit">leggja fram</button>
</form>
```

## stílaflokkur

### `.Ing`
- Staða bendilsins er `wait`.
- Gerviþáttur `::before` gefur gára hreyfimynd.
- Þegar það er notað á `form` einingunni gefur gerviþátturinn `::after` fram 1px óskýra grímu, sem gerir samskipti og textaval óvirkt.