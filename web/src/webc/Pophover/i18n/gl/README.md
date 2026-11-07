# aviso de burbulla flotante

Mostra a información sobre ferramentas do rato cando se pasa o rato ou se activa o elemento.

- Baseado en compoñente web nativo, lixeiro e de alto rendemento
- Admite o posicionamento espacial adaptativo en catro direccións (clase = "arriba/abaixo/esquerda/dereita")
- Usa un fondo degradado de textura de vidro esmerilado negro e caracteres brancos, con relámpagos de contorno físico exquisitos.
- Cálculo adaptativo de posición incorporado, axusta automaticamente a dirección emerxente segundo o espazo restante na ventana gráfica
- Use `slot="pop"` para mostrar contido de solicitude flotante

## Use a demostración

```html
<!-- Calcula automaticamente a dirección (recomendado, non é necesario declarar a clase) -->
<c-pophover>
  <button>automático</button>
  <div slot="pop">Esta é unha punta de burbulla</div>
</c-pophover>

<!-- dirección fixa -->
<c-pophover class="left">
  <button>Esquerda</button>
  <div slot="pop">Esta é unha punta de burbulla</div>
</c-pophover>

<c-pophover class="right">
  <button>certo</button>
  <div slot="pop">Esta é unha punta de burbulla</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Abaixo</button>
  <div slot="pop">Esta é unha punta de burbulla</div>
</c-pophover>

<c-pophover class="top">
  <button>superior</button>
  <div slot="pop">Esta é unha punta de burbulla</div>
</c-pophover>
```

## Variables CSS

Os estilos personalizados son compatibles coas seguintes variables CSS:

| nome da variable | valor predeterminado | ilustrar |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | O espazo entre a información da burbulla e o elemento desencadeador |
| `--pophover-margin` | `24px` | Marxe de seguridade do bordo da vista durante o cálculo de posicionamento adaptativo |
| `--pophover-bg-top` | `#18181ce0` | Cor de fondo degradado da burbulla |
| `--pophover-bg-mid` | `#121215e5` | A cor de fondo das frechas dos lados esquerdo e dereito da burbulla |
| `--pophover-bg-bottom` | `#0c0c0feb` | A cor das frechas laterais inferior e superior e inferior do fondo degradado da burbulla |

## Estilo de información sobre ferramentas da icona (`.Ico`)

Para os botóns de aviso flotantes de tipo de icona puro (como "Editar", "Eliminar", etc.), pode aplicar unha icona unificada e un estilo de pasar o rato engadindo o nome da clase `.Ico` ao propio contenedor ou compoñente exterior.

### Exemplo de estrutura

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">editar</div>
  </c-pophover>
</b>
```

O deseño básico, o tamaño (18 x 18 px) e os cambios de cor ao pasar o rato unificáronse en `.Ico > c-pophover > a`. Os desenvolvedores só precisan especificar o ficheiro de icona correspondente por separado, como:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```