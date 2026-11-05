# barra di scorrimento virtuale

- **Scorrimento bidirezionale**: fornisce lo scorrimento verticale (`c-vs`) e orizzontale (`c-hs`).
- **Risposta dimensioni**: monitora le modifiche alle dimensioni del contenuto e dell'area, aggiorna automaticamente le dimensioni e la posizione della barra di scorrimento e nascondi automaticamente quando non si verifica alcun overflow.
- **Supporto interattivo**: supporta lo scorrimento della rotellina, il trascinamento del cursore e il posizionamento del clic sulla traccia.
- **Nascondi stato**: mostra e allarga il cursore quando si passa il mouse o si scorre e lo nasconde automaticamente quando si è fermi.

## scorrimento verticale

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## scorrimento orizzontale

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## parte di stile

Personalizza lo stile degli elementi interni di Shadow DOM tramite `::part()`:

- `::part(scroll)`: scorri l'area dei contenuti.
- `::part(bar)`: traccia della barra di scorrimento.
- `::part(si)`: cursore della barra di scorrimento.
- `::part(i)`: Strato di rivestimento interno.
- `::part(drag)`: la traccia della barra di scorrimento nello stato di trascinamento.

## lezione di stile

- `.drag`: applicato all'elemento `body`, indica che è in uno stato di scorrimento tramite trascinamento.

## Proprietà personalizzate

- `--si-bg`: colore di sfondo del dispositivo di scorrimento.
- `--si-anim`: animazione del dispositivo di scorrimento.
- `--cursorScrollhSvg`: il cursore del mouse quando si passa sulla barra di scorrimento orizzontale.
- `--cursorScrollvSvg`: il cursore del mouse quando si passa sulla barra di scorrimento verticale.