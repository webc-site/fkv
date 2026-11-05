# Indicateur d'état de chargement ou de traitement

- Unifiez le curseur en état d'attente
- Affiche une animation d'ondulation pour indiquer l'état du traitement
- Appliquer un masque de flou à un formulaire pour éviter les soumissions en double

## Utilisez la démo

```html
<!-- indicateur de chargement indépendant -->
<div class="Ing"></div>

<!-- Formulaire de chargement avec masque -->
<form class="Ing">
  <input type="text" placeholder="Zone de saisie">
  <button type="submit">soumettre</button>
</form>
```

## cours de style

### `.Ing`
- L'état du pointeur est `wait`.
- Le pseudo-élément `::before` restitue une animation d'ondulation.
- Lorsqu'il est utilisé sur l'élément `form`, le pseudo-élément `::after` restitue un masque de flou de 1 px, désactivant l'interaction et la sélection de texte.