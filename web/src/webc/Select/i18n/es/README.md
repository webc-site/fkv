# Selección desplegable, el ancho se adapta al elemento actual

El ancho del `select` nativo está determinado por el texto de opción más largo de forma predeterminada. Al cambiar de opciones cortas, hay demasiado espacio en blanco, lo que afecta la apariencia.

Este componente cubre de forma transparente el `select` nativo encima de la etiqueta `b`, lo que permite que el ancho se adapte con precisión a la longitud del texto seleccionado actualmente.

- Mantener estilos de menú desplegable nativos `select` y soporte de accesibilidad en todos los terminales
- Admite operaciones interactivas nativas, como navegación completa por pestañas del teclado y cambio de teclas de dirección.
- Adapte el ancho a la opción actual para eliminar el exceso de espacio en blanco.

## Usa la demostración

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```