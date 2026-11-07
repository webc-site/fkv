# Pestañas intercambiables

- El estado seleccionado de la etiqueta se integra perfectamente con el área de contenido a continuación.
- Especificaciones estructurales: los elementos secundarios están fijos para usar `nav > a` como etiqueta de navegación, y el elemento secundario directo `b` se usa como área de contenido.
- Asocie automáticamente la etiqueta `a[value]` con el nodo de contenido `b[slot]`.
- Admite la conservación automática de la página activa actual en `localStorage` a través del atributo `key`.
- Admite especificar la página de la pestaña activada a través del atributo `value` (si no hay caché y `value`, la etiqueta con `class="A"` o la primera etiqueta se seleccionará de forma predeterminada).
- Implementación ligera de Light DOM, sin obstrucción de Shadow DOM, alto grado de libertad en la personalización del estilo.

## Usa la demostración

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">manzana</a>
    <a value="banana">banana</a>
    <a value="orange">Mandarina</a>
  </nav>
  <b slot="apple">La manzana es una fruta nutritiva</b>
  <b slot="banana">El plátano es una fruta tropical.</b>
  <b slot="orange">Las naranjas son ricas en vitamina C.</b>
</c-tab>
```

## propiedad

- `key`: nombre de clave utilizado para la persistencia del almacenamiento local
- `value`: valor seleccionado actualmente

## Eventos personalizados

- `change`: se activa cuando se cambia de pestaña, `e.value` es el valor `value` de la etiqueta actualmente activada.