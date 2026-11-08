# Selecció desplegable, l'amplada s'adapta a l'element actual

L'amplada del `select` natiu ve determinada pel text d'opció més llarg de manera predeterminada. Quan canvieu les opcions curtes, hi ha massa espai en blanc, cosa que afecta l'aparença.

Aquest component cobreix de manera transparent l'`select` natiu a sobre de l'etiqueta `b`, permetent que l'amplada s'adapti amb precisió a la longitud del text seleccionat actualment.

- Manteniu els estils de menú desplegable `select` natius i el suport d'accessibilitat a tots els terminals
- Admet operacions interactives natives, com ara la navegació completa de la pestanya del teclat i el canvi de tecles de direcció.
- Adapta l'amplada a l'opció actual per eliminar l'excés d'espai en blanc

## Utilitzeu la demostració

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```