# Navigační nabídka přizpůsobená různým velikostem obrazovky

- Plocha: Odkazy se zobrazují ve vodorovných dlaždicích
- Mobilní verze: Sbalte do tlačítka nabídky, kliknutím vysuňte zásuvku bočního panelu
- Aktivace trasy: automaticky porovnejte aktuální cestu a hodnotu hash a přidejte název třídy stavu aktivace k odpovídajícímu odkazu `A`
- Automaticky zavřít: Když mobilní terminál klikne na odkaz nebo externí pozadí postranního panelu, postranní panel se automaticky zavře.

## použití

```html
<c-nav-menu>
  <a href="/">titulní strana</a>
  <a href="/doc">dokument</a>
  <a href="/about">o</a>
</c-nav-menu>
```

## ilustrovat

Když je komponenta inicializována, pokud v podřízeném prvku není `c-side`, automaticky se vytvoří tlačítko mobilní nabídky a postranní panel `c-side` a všechny značky `a` se přesunou do postranního panelu.

## stylová třída

- **A**: Aktivovaný odkaz automaticky přidá název třídy `A`