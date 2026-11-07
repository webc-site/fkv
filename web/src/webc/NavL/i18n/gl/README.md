# Cambia automaticamente o menú de navegación e o botón de retorno

- Envolve automaticamente os elementos fillos para xerar estruturas de menú de navegación e botón de retorno
- Rastrexa automaticamente o historial de rutas e calcula a ruta de retorno anterior
- Mostra o menú de navegación cando a ruta coincide co elemento do menú; se non, amosa o botón Atrás

## Use a demostración

```html
<c-nav-l>
  <a href="/">primeira páxina</a>
  <a href="/doc">documento</a>
</c-nav-l>
```

## clase de estilo

- `B`: activa o estado do botón de retorno. Mostra o botón Atrás e oculta o menú de navegación. Cambio automático dentro do compoñente en función do enrutamento.