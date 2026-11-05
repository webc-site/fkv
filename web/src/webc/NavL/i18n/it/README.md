# Cambia automaticamente il menu di navigazione e il pulsante Indietro

- Avvolgi automaticamente gli elementi secondari per generare menu di navigazione e restituire strutture di pulsanti
- Traccia automaticamente la cronologia dei percorsi e calcola il percorso di ritorno precedente
- Visualizza il menu di navigazione quando il percorso corrisponde alla voce di menu, altrimenti visualizza il pulsante Indietro

## Utilizza la demo

```html
<c-nav-l>
  <a href="/">prima pagina</a>
  <a href="/doc">documento</a>
</c-nav-l>
```

## lezione di stile

- `B`: attiva lo stato del pulsante di ritorno. Mostra il pulsante Indietro e nascondi il menu di navigazione. Commutato automaticamente all'interno del componente in base al routing.