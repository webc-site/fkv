# Renderizar e mostrar demostracións de compoñentes, documentación e instrucións de instalación

- Extrae e analiza o título, a introdución e o contido do documento README
- Sandbox para executar demostracións interactivas de compoñentes e illar estilos
- Converte bloques de código en documentos en presentacións interactivas
- Disposición adaptativa, visualización de columnas de pantalla ancha, visualización de cambio de pestanas de pantalla estreita

## Use a demostración

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializar e obter a interface de operación
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parámetros da interface

### `WebCDemo(el)`

Inicializa o compoñente.

- **parámetro**
  - `el`:`HTMLElement`, contedor de compoñentes.
- **Valor de retorno**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: configura o módulo de demostración.
  - `setMd(name, md, lang)`: define o nome do compoñente, o contido do documento README e o idioma actual.
  - `unMount()`: descarga o compoñente e limpa a escoita de eventos.

### Módulo de demostración `mod` atributo

- `CSS`:`string`, o camiño da folla de estilo do compoñente.
- `HTM`:`string`, o modelo HTML do compoñente.
- `default`:`function(root)`, función de inicialización de compoñentes, recibe o contenedor raíz sombra.

## clase de estilo

### `.M`

área de contido.

### `.L`

O panel esquerdo contén instrucións e documentación.

### `.R`

Panel dereito, que contén a demostración.

### `c-nav`

Barra de navegación de pantalla estreita.