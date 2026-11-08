# Estilos de tipografía optimizados para la experiencia de lectura.

- **Composición tipográfica básica**: Adaptarse a títulos, párrafos, negrita, cursiva, tachado, código en línea, imágenes y líneas divisorias.
- **Animación del enlace**: muestra una animación subrayada al pasar el cursor.
- **Lista de proyectos**: las listas desordenadas usan viñetas, las listas ordenadas usan números de serie de fondo redondeados y admiten anidamiento de varios niveles.
- **Lista de tareas**: proporciona estilo de casilla de verificación, admite tareas pendientes y estado completado.
- **Consejos de advertencia**: Admite cinco tipos de cuadros de advertencia: Nota, Consejo, Importante, Advertencia y Atención.
- **Tablas y código**: proporciona resaltado de filas de tabla y bloques de código con sombras y fondos degradados.

## Usa la demostración

```html
<div class="Md">
  <h1>título</h1>
  <p>Texto de párrafo y <a href="#">enlace</a></p>
  <hr>
  <h2>Título de segundo nivel</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Elemento de tarea</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Contenido de la nota</p>
  </blockquote>
</div>
```

## clase de estilo

Los estilos se aplican a elementos HTML bajo el contenedor `.Md`.

### Estilo de cuadro de alerta

Agregue el siguiente nombre de clase en el elemento `blockquote` con el nombre de clase `.q` para alternar el cuadro de alerta:

- `note`: Notas
- `tip`: Consejo
- `important`: Importante
- `warning`: advertencia
- `caution`: Nota