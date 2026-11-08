# Documentación e código fonte para renderizar compoñentes ou funcións da ferramenta

- Disposición receptiva de dúas columnas, con documentos mostrados á esquerda e código fonte mostrado á dereita
- Soporte de documentación e código fonte Botóns de acceso directo Editar e Código para ir ao enderezo fonte.
- O título e a introdución móstranse na parte superior e admiten a tradución automática de i18n.

## Use a demostración

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parámetros da propiedade

- `src`: enderezo URL do código fonte.
- `href`: enderezo URL do documento marcado, admite o uso de `{code}` marcador de posición para adaptarse ao idioma actual.