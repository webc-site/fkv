# Menu de navigation adapté aux différentes tailles d'écran

- Bureau : les liens sont affichés dans des vignettes horizontales
- Version mobile : Réduire dans le bouton de menu, cliquez pour faire glisser le tiroir de la barre latérale
- Activation de la route : comparez automatiquement le chemin actuel et la valeur de hachage, et ajoutez le nom de la classe d'état d'activation au lien correspondant `A`
- Fermer automatiquement : Lorsque le terminal mobile clique sur un lien ou sur le fond externe de la barre latérale, la barre latérale se ferme automatiquement.

## utiliser

```html
<c-nav-menu>
  <a href="/">première page</a>
  <a href="/doc">document</a>
  <a href="/about">à propos</a>
</c-nav-menu>
```

## illustrer

Lorsque le composant est initialisé, s'il n'y a pas de `c-side` dans l'élément enfant, le bouton de menu mobile et la barre latérale `c-side` seront automatiquement créés, et toutes les balises `a` seront déplacées dans la barre latérale.

## cours de style

- **A** : Le lien activé ajoutera automatiquement le nom de la classe `A`