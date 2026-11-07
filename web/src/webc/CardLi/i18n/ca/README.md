# Carregueu i visualitzeu la llista de targetes de manera asíncrona

- **Càrrega asíncrona**: obteniu dades mitjançant una funció asíncrona
- **Visualització d'estat**: admet tres estats: càrrega, dades buides i targeta de representació.
- **Disposició flexible**: les targetes es disposen en files

## Utilitzeu la demostració

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Títol " + i, "Descripció" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Paràmetres de la interfície

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funció de càrrega de dades asíncrona. El format de cada subclau és `[nom, introducció, enllaç]`.

## classe d'estil

- `.CardLi`: element exterior
- `.Card`: element de la targeta
- `.Ing`: s'està carregant l'element animat