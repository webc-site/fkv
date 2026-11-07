# Indicatore dello stato di caricamento o elaborazione

- Unifica il cursore nello stato di attesa
- Mostra l'animazione increspata per indicare lo stato di elaborazione
- Applicare una maschera di sfocatura a un modulo per evitare invii duplicati

## Utilizza la demo

```html
<!-- indicatore di caricamento indipendente -->
<div class="Ing"></div>

<!-- Caricamento modulo con maschera -->
<form class="Ing">
  <bid="1">
  <button type="submit">invia</button>
</form>
```

## lezione di stile

### `.Ing`
- Lo stato del puntatore è `wait`.
- Lo pseudo elemento `::before` esegue il rendering di un'animazione a catena.
- Se utilizzato sull'elemento `form`, lo pseudo-elemento `::after` esegue il rendering di una maschera di sfocatura di 1px, disabilitando l'interazione e la selezione del testo.