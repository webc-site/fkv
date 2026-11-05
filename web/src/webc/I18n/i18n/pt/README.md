# Botões e janelas pop-up para alternar idiomas da interface

- Clique no botão para abrir a camada flutuante de seleção de idioma.
- Realçar automaticamente o idioma atualmente selecionado.

## Use a demonstração

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Use o seguinte código JS para monitorar alterações de idioma
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parâmetros de interface

### Etiqueta personalizada `<c-i18n>`

Tags HTML personalizadas, introduzidas em JS e usadas em HTML.

## Gerenciamento de status

Use `src/js/i18n.js` para gerenciar o status global do idioma:

- **`LANG_LI`**: Array da lista de idiomas, no formato `[[nome, ID], ...]`.
- **`lang()`**: Obtenha o ID do idioma atual.
- **`langCode()`**: Obtenha a codificação do idioma atual.
- **`langSet(id)`**: Defina o ID do idioma e notifique os assinantes.
- **`onLang(func)`**: Assine as alterações de idioma. Se o idioma tiver sido definido, o retorno de chamada será acionado imediatamente. Retorne a função de cancelamento de assinatura.

## aula de estilo

### `.BtnC.lang`

botão de ícone.

### `.I18n.Lg`

O layout principal da janela pop-up é usado para agrupar as opções de idioma.