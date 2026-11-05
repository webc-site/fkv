# Selección despregable, o ancho adáptase ao elemento actual

O ancho do `select` nativo está determinado polo texto de opción máis longo de forma predeterminada. Ao cambiar as opcións curtas, hai demasiado espazo en branco, o que afecta a aparencia.

Este compoñente cobre de forma transparente o `select` nativo por riba da etiqueta `b`, o que permite que o ancho se adapte con precisión á lonxitude do texto seleccionado actualmente.

- Mantén os estilos de menú despregable `select` nativos e a compatibilidade de accesibilidade en todos os terminais
- Admite operacións interactivas nativas, como a navegación completa de pestanas do teclado e o cambio de teclas de dirección.
- Adapta o ancho á opción actual para eliminar o exceso de espazo en branco

## Use a demostración

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```