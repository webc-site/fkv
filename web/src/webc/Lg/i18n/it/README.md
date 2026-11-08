# Sfondo in vetro smerigliato ed effetto evidenziatore del bordo

Il bordo semitrasparente e l'ombra interna creano punti luce sui bordi in microrilievo.
Come dipendenza di stile di base dei componenti pulsante e casella di input.

## Utilizza la demo

```html
<main class="Lg">
  <article>
    <h3>titolo</h3>
    <pre>Testo del contenuto</pre>
  </article>
</main>
```

## lezione di stile

### `.Lg`
Stile base della carta. Applica la `backdrop-filter` sfocatura, il `border` bordo in microrilievo e l'`box-shadow` ombra interna. Applica il filtro `svg/glass.svg#h` al passaggio del mouse.

### `main.Lg`
Il contenitore di blocchi principale della pagina. Angoli arrotondati `24px` con riempimento reattivo (`16px` se la larghezza è inferiore a `800px`, `24px` altrimenti).

### `.Btn`, `.BtnC`, `.Input`
Nome della classe del componente di base. Dopo aver introdotto questo stile, otterrai automaticamente layout flessibile, relazioni gerarchiche e proprietà di base del vetro smerigliato.