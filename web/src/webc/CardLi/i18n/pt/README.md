# Carregar e exibir lista de cartões de forma assíncrona

- **Carregamento assíncrono**: obtenha dados por meio de uma função assíncrona
- **Exibição de status**: suporta três estados: carregamento, dados vazios e cartão de renderização.
- **Layout flexível**: os cartões são organizados em linhas

## Use a demonstração

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Título " + i, "Descrição " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parâmetros de interface

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, função de carregamento assíncrono de dados. O formato de cada subchave é `[nome, introdução, link]`.

## aula de estilo

- `.CardLi`: elemento externo
- `.Card`: elemento cartão
- `.Ing`: Carregando elemento animado