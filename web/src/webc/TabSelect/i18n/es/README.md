# Página de pestaña con selección desplegable

- Admite la incrustación del cuadro de selección desplegable de ancho adaptable (`<c-select>`) en la etiqueta, que solo se mostrará cuando la página de pestaña actual esté activada.
- Especificación estructural: el elemento secundario utiliza `nav > a` como etiqueta de navegación y el elemento secundario directo `b` sirve como área de contenido.
- Asocia automáticamente la etiqueta `a[value]`, `a` con `c-select` (según el atributo de valor del grupo y el valor de la opción seleccionada) y el nodo de contenido `b[slot]`.
- Admite la persistencia automática del elemento actualmente activado en `localStorage` a través del atributo `key`.
- Admite el guardado y la restauración automáticos de subselecciones históricas para cada grupo en `localStorage` usando `key + '/' + tab`.
- Admite especificar la pestaña activa a través del atributo `value`.

## Usa la demostración

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>idioma</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistema</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript es un lenguaje de secuencias de comandos</b>
    <b slot="rs">Rust es un lenguaje de programación a nivel de sistemas</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS es un sistema operativo gráfico basado en Unix</b>
    <b slot="win">Windows es un sistema operativo desarrollado por Microsoft</b>
  </b>
</c-tab-select>
```

## propiedad

- `key`: nombre de clave utilizado para la persistencia del almacenamiento local
- `value`: valor seleccionado actualmente

## Eventos personalizados

- `change`: Se activa al cambiar de pestaña, `e.value` es el valor `value` de la opción actualmente activada.