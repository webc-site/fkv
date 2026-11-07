# Menú de navegación adaptado a diferentes tamaños de pantalla

- Escritorio: los enlaces se muestran en mosaicos horizontales
- Versión móvil: contraer en el botón de menú, hacer clic para deslizar el cajón de la barra lateral
- Activación de ruta: compare automáticamente la ruta actual y el valor hash, y agregue el nombre de la clase de estado de activación al enlace correspondiente `A`
- Cerrar automáticamente: Cuando el terminal móvil hace clic en un enlace o en el fondo externo de la barra lateral, la barra lateral se cierra automáticamente.

## usar

```html
<c-nav-menu>
  <a href="/">página delantera</a>
  <a href="/doc">documento</a>
  <a href="/about">acerca de</a>
</c-nav-menu>
```

## ilustrar

Cuando se inicializa el componente, si no hay `c-side` en el elemento secundario, el botón del menú móvil y la barra lateral `c-side` se crearán automáticamente, y todas las etiquetas `a` se moverán a la barra lateral.

## clase de estilo

- **A**: El enlace activado agregará automáticamente el nombre de la clase `A`