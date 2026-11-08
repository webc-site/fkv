# mensaje de burbuja flotante

Muestra información de información sobre herramientas al pasar el mouse o cuando se activa el elemento.

- Basado en Web Component nativo, liviano y de alto rendimiento.
- Admite posicionamiento espacial adaptativo en cuatro direcciones (class="top/bottom/left/right")
- Utiliza un fondo degradado de textura de vidrio esmerilado negro y caracteres blancos, con exquisitos reflejos físicos en los contornos de los bordes.
- Cálculo adaptativo de posición incorporado, ajusta automáticamente la dirección emergente de acuerdo con el espacio restante en la ventana gráfica
- Utilice `slot="pop"` para mostrar contenido de aviso flotante

## Usa la demostración

```html
<!-- Calcular automáticamente la dirección (recomendado, no es necesario declarar la clase) -->
<c-pophover>
  <button>automático</button>
  <div slot="pop">Esta es una punta de burbuja.</div>
</c-pophover>

<!-- dirección fija -->
<c-pophover class="left">
  <button>Izquierda</button>
  <div slot="pop">Esta es una punta de burbuja.</div>
</c-pophover>

<c-pophover class="right">
  <button>bien</button>
  <div slot="pop">Esta es una punta de burbuja.</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Abajo</button>
  <div slot="pop">Esta es una punta de burbuja.</div>
</c-pophover>

<c-pophover class="top">
  <button>superior</button>
  <div slot="pop">Esta es una punta de burbuja.</div>
</c-pophover>
```

## variables CSS

Los estilos personalizados se admiten a través de las siguientes variables CSS:

| nombre de la variable | valor predeterminado | ilustrar |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | El espacio entre la información sobre herramientas de la burbuja y el elemento desencadenante. |
| `--pophover-margin` | `24px` | Margen de seguridad del borde de la ventana gráfica durante el cálculo de posicionamiento adaptativo |
| `--pophover-bg-top` | `#18181ce0` | Color superior del fondo degradado de burbuja |
| `--pophover-bg-mid` | `#121215e5` | El color de fondo de las flechas en los lados izquierdo y derecho de la burbuja. |
| `--pophover-bg-bottom` | `#0c0c0feb` | El color de las flechas inferior y superior e inferior del fondo degradado de la burbuja |

## Estilo de información sobre herramientas del icono (`.Ico`)

Para botones flotantes de tipo ícono puro (como "Editar", "Eliminar", etc.), puede aplicar un ícono unificado y un estilo de desplazamiento agregando el nombre de clase `.Ico` al contenedor externo o al componente mismo.

### Ejemplo de estructura

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">editar</div>
  </c-pophover>
</b>
```

El diseño básico, el tamaño (18x18px) y los cambios de color al pasar el cursor se han unificado en `.Ico > c-pophover > a`. Los desarrolladores solo necesitan especificar el archivo de icono correspondiente por separado, como por ejemplo:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```