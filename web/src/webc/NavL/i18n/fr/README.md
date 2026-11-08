# Changer automatiquement le menu de navigation et le bouton de retour

- Encapsuler automatiquement les éléments enfants pour générer des structures de menu de navigation et de bouton de retour
- Tracez automatiquement l'historique de routage et calculez le chemin de retour précédent
- Afficher le menu de navigation lorsque l'itinéraire correspond à l'élément de menu, sinon afficher le bouton retour

## Utilisez la démo

```html
<c-nav-l>
  <a href="/">première page</a>
  <a href="/doc">document</a>
</c-nav-l>
```

## cours de style

- `B` : active l'état du bouton de retour. Afficher le bouton de retour et masquer le menu de navigation. Commutation automatique au sein du composant en fonction du routage.