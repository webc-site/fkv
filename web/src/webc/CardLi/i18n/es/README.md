# Cargar y mostrar la lista de tarjetas de forma asincrónica

- **Carga asincrónica**: obtenga datos a través de una función asincrónica
- **Visualización de estado**: Admite tres estados: cargando, datos vacíos y tarjeta de representación.
- **Diseño flexible**: las tarjetas están dispuestas en filas

## Usa la demostración

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Título " + i, "Descripción " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parámetros de interfaz

### CardLi(loader)

- **cargador**: `() => Promise<Array<[string, string, string]>>`, función de carga de datos asíncrona. El formato de cada subclave es `[nombre, introducción, enlace]`.

## clase de estilo

- `.CardLi`: elemento exterior
- `.Card`: elemento de tarjeta
- `.Ing`: Cargando elemento animado