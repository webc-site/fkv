# Analiza e renderiza de forma segura o texto de Markdown en HTML.

- **Desinfección de seguridade**: defende contra ataques XSS usando API nativas `setHTML` ou produtos de limpeza `DOMParser`
- **Extracción de contido**: le automaticamente o contido de texto do espazo predeterminado durante a inicialización e renderízao
- **Composición integrada**: estilos de lista predefinida, táboa, bloque de código, cita e caixa de aviso

## Use a demostración

### renderizado estático

```html
<c-md>
# título
Esta é unha peza de contido Markdown
</c-md>
```

### asignación dinámica

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Novo título\nNovo contido";
</script>
```

## Parámetros da interface

Pasa texto de Markdown directamente dentro da etiqueta (como espazo predeterminado) ou actualiza de forma dinámica a través de `value`.

## Descrición do estilo

A clase de estilo `.Md` engádese automaticamente cando se inicializa o compoñente. Admite o seguinte deseño de caixa de advertencia estendida de Markdown (activado pola sintaxe de referencia):

- `[!NOTE]`：Información rápida
- `[!TIP]`: suxestións/suxestións
- `[!IMPORTANT]`: atención importante
- `[!WARNING]`: aviso de risco
- `[!CAUTION]`: aviso de risco moi alto

## mecanismo de seguridade

Ao renderizar, chame primeiro á API nativa `setHTML` para inserir HTML de forma segura. Se o navegador non o admite, baixará automaticamente a un limpador recursivo baseado en `DOMParser` e implementará a seguinte política de seguranza:
- Elimina as etiquetas non seguras (por exemplo, `script`, `iframe`, etc.)
- Borrar `on*` atributos do evento
- Defende os ataques XSS filtrando atributos que comezan con protocolos perigosos como `javascript:`, `vbscript:` e `data:` (excepto `data:image/`).