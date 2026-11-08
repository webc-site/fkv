# Estilos de tipografia otimizados para experiência de leitura

- **Composição tipográfica básica**: Adapte-se a títulos, parágrafos, negrito, itálico, tachado, código embutido, imagens e linhas divisórias.
- **Animação de link**: exibe animação de sublinhado ao passar o mouse.
- **Lista de projetos**: listas não ordenadas usam marcadores, listas ordenadas usam números de série de fundo arredondados e suportam aninhamento de vários níveis.
- **Lista de tarefas**: fornece estilo de caixa de seleção, oferece suporte a tarefas e status concluído.
- **Dicas de aviso**: Suporta cinco tipos de caixas de aviso: Nota, Dica, Importante, Aviso e Atenção.
- **Tabelas e código**: fornece destaque de linha da tabela e blocos de código com sombras e planos de fundo gradientes.

## Use a demonstração

```html
<div class="Md">
  <h1>título</h1>
  <p>Texto do parágrafo e <a href="#">link</a></p>
  <hr>
  <h2>Título de segundo nível</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Item de Tarefa</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Conteúdo da nota</p>
  </blockquote>
</div>
```

## aula de estilo

Os estilos são aplicados aos elementos HTML no contêiner `.Md`.

### Estilo de caixa de alerta

Adicione o seguinte nome de classe no elemento `blockquote` com o nome de classe `.q` para alternar a caixa de alerta:

- `note`: Notas
- `tip`: Dica
- `important`: Importante
- `warning`: aviso
- `caution`: Observação