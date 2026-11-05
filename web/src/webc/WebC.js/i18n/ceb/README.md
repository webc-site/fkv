# Dokumentasyon ug source code alang sa paghubad sa mga sangkap o gamit sa himan

- Duha ka kolum nga responsive nga layout, nga adunay mga dokumento nga gipakita sa wala ug source code nga gipakita sa tuo
- Suporta sa dokumentasyon ug source code ang Edit ug Code shortcut nga mga buton aron moambak ngadto sa source address.
- Ang titulo ug pasiuna gipakita sa taas, nga nagsuporta sa awtomatikong paghubad sa i18n.

## Gamita ang demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Mga parameter sa kabtangan

- `src`: URL address sa source code.
- `href`: URL address sa markdown sa dokumento, nagsuporta gamit ang `{code}` placeholder aron ipahiangay sa kasamtangan nga pinulongan.