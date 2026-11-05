# Representar y mostrar demostraciones de componentes, documentación e instrucciones de instalación.

- Extraiga y analice el título, la introducción y el contenido del documento README.
- Sandbox para ejecutar demostraciones interactivas de componentes y aislar estilos
- Convierta bloques de código en documentos en presentaciones interactivas
- Diseño adaptable, visualización de columnas en pantalla ancha, visualización de cambio de pestañas en pantalla estrecha

## Usa la demostración

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializa y obtiene la interfaz de operación
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parámetros de interfaz

### `WebCDemo(el)`

Inicialice el componente.

- **parámetro**
  - `el`:`HTMLElement`, contenedor de componentes.
- **Valor de retorno**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Configure el módulo de demostración.
  - `setMd(name, md, lang)`: establece el nombre del componente, el contenido del documento README y el idioma actual.
  - `unMount()`: descargue el componente y limpie el detector de eventos.

### Atributo del módulo de demostración `mod`

- `CSS`:`string`, la ruta de la hoja de estilos del componente.
- `HTM`:`string`, la plantilla HTML del componente.
- `default`:`function(root)`, función de inicialización del componente, recibe el contenedor raíz oculto.

## clase de estilo

### `.M`

área de contenido.

### `.L`

El panel izquierdo contiene instrucciones y documentación.

### `.R`

Panel derecho, que contiene la demostración.

### `c-nav`

Barra de navegación de pantalla estrecha.