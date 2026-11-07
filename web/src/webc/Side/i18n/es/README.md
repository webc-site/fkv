# Paneles de cajones que se deslizan hacia afuera desde el costado

- Crear automáticamente estructuras de desplazamiento interno
- Haga clic en el área exterior para contraer automáticamente
- Desliza el gesto hacia la izquierda y hacia la derecha para cerrar
- Posicionamiento de deslizamiento lateral en direcciones izquierda y derecha.
- Proporcionar eventos de apertura y cierre.

## Usa la demostración

```html
<button class="open-btn">Abierto</button>

<c-side class="right">
  <div>Contenido del cajón</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interfaz

### método

- `open(el)`: Función de exportación, abrir cajón
- `close(el)`: Función exportar, cerrar cajón
- `el.open()`: método de instancia, abrir cajón
- `el.close()`: método de instancia, cierra el cajón

### evento

- `open`: Se dispara cuando se enciende
- `close`: Se dispara cuando está cerrado

## clase de estilo

- `.right`: posicionamiento deslizable hacia la derecha
- `.top`: Posicionamiento deslizante
- `.bottom`: deslizarse fuera de posición
- (Si no está configurado, el valor predeterminado es deslizarse hacia la izquierda)
- `.open`: estado abierto
- `.H`: estado oculto
- `.T`: estado de transición de animación