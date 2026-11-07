# Hlaða og birta kortalista ósamstillt

- **Ósamstilltur hleðsla**: Fáðu gögn með ósamstilltri aðgerð
- **Stöðuskjár**: Styður þrjár stöður: hleðsla, tóm gögn og endurgerð kort.
- **Sveigjanlegt útlit**: Spilum er raðað í raðir

## Notaðu kynninguna

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titill " + i, "Lýsing " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Viðmótsbreytur

### CardLi(loader)

- **hleðslutæki**: `() => Promise<Array<[string, string, string]>>`, ósamstilltur gagnahleðsluaðgerð. Snið hvers undirlykils er `[nafn, inngangur, tengill]`.

## stílaflokkur

- `.CardLi`: ytri þáttur
- `.Card`: kortaþáttur
- `.Ing`: Hleður hreyfimynd