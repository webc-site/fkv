# Cambiar automáticamente el menú de navegación y el botón de retorno

- Ajustar automáticamente elementos secundarios para generar menús de navegación y estructuras de botones de retorno
- Rastree automáticamente el historial de rutas y calcule la ruta de retorno anterior
- Mostrar el menú de navegación cuando la ruta coincida con el elemento del menú; de lo contrario, mostrar el botón Atrás

## Usa la demostración

```html
<c-nav-l>
  <a href="/">página delantera</a>
  <a href="/doc">documento</a>
</c-nav-l>
```

## clase de estilo

- `B`: activa el estado del botón de retorno. Muestra el botón Atrás y oculta el menú de navegación. Cambia automáticamente dentro del componente según el enrutamiento.