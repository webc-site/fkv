# Cargar e mostrar a lista de tarxetas de forma asíncrona

- **Carga asíncrona**: obtén datos mediante unha función asíncrona
- **Visualización de estado**: admite tres estados: carga, datos baleiros e tarxeta de renderizado.
- **Disposición flexible**: as tarxetas están dispostas en filas

## Use a demostración

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Título " + i, "Descrición" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parámetros da interface

### CardLi(loader)

- **cargador**: `() => Promise<Array<[string, string, string]>>`, función de carga de datos asíncrona. O formato de cada subclave é `[nome, introdución, ligazón]`.

## clase de estilo

- `.CardLi`: elemento exterior
- `.Card`: elemento da tarxeta
- `.Ing`: cargando elemento animado