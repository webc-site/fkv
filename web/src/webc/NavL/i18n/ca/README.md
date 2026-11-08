# Canvia automàticament el menú de navegació i el botó de retorn

- Embolica automàticament els elements secundaris per generar estructures de menú de navegació i botons de retorn
- Traceu automàticament l'historial d'encaminament i calculeu el camí de retorn anterior
- Mostra el menú de navegació quan la ruta coincideix amb l'element del menú, en cas contrari, mostra el botó enrere

## Utilitzeu la demostració

```html
<c-nav-l>
  <a href="/">portada</a>
  <a href="/doc">document</a>
</c-nav-l>
```

## classe d'estil

- `B`: activa l'estat del botó de retorn. Mostra el botó enrere i amaga el menú de navegació. Canviat automàticament dins del component en funció de l'encaminament.