# Estilos tipográficos optimizados para a experiencia lectora

- **Composición básica**: Adáptase a títulos, parágrafos, negriña, cursiva, tachado, código en liña, imaxes e liñas divisorias.
- **Animación da ligazón**: mostra a animación de subliñado ao pasar o rato.
- **Lista de proxectos**: as listas non ordenadas usan viñetas, as listas ordenadas usan números de serie de fondo redondeados e admiten a anidación de varios niveis.
- **Lista de tarefas**: ofrece un estilo de caixa de verificación, admite tarefas pendentes e estado completado.
- **Consellos de advertencia**: admite cinco tipos de caixas de advertencia: Nota, Consello, Importante, Aviso e Atención.
- **Táboas e código**: ofrece resaltado de filas de táboas e bloques de código con sombras e fondos degradados.

## Use a demostración

```html
<div class="Md">
  <h1>título</h1>
  <p>Texto do parágrafo e <a href="#">ligazón</a></p>
  <hr>
  <h2>Título de segundo nivel</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Elemento de tarefa</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Contido da nota</p>
  </blockquote>
</div>
```

## clase de estilo

Os estilos aplícanse aos elementos HTML no contedor `.Md`.

### Estilo de caixa de alerta

Engade o seguinte nome de clase no elemento `blockquote` co nome de clase `.q` para activar o cadro de alerta:

- `note`: Notas
- `tip`: Consello
- `important`: importante
- `warning`: aviso
- `caution`: Nota