# Menú de navegación adaptado a diferentes tamaños de pantalla

- Escritorio: as ligazóns móstranse en mosaicos horizontais
- Versión móbil: contrae o botón de menú, fai clic para saír do caixón da barra lateral
- Activación da ruta: compara automaticamente a ruta actual e o valor hash e engade o nome da clase de estado de activación á ligazón coincidente `A`
- Pechar automaticamente: cando o terminal móbil fai clic nunha ligazón ou no fondo externo da barra lateral, a barra lateral pecharase automaticamente.

## usar

```html
<c-nav-menu>
  <a href="/">primeira páxina</a>
  <a href="/doc">documento</a>
  <a href="/about">sobre</a>
</c-nav-menu>
```

## ilustrar

Cando se inicialice o compoñente, se non hai `c-side` no elemento secundario, crearanse automaticamente o botón do menú móbil e a barra lateral `c-side` e todas as etiquetas `a` moveranse á barra lateral.

## clase de estilo

- **A**: a ligazón activada engadirá automaticamente o nome da clase `A`