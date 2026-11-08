# Barre de navigation supérieure qui se réduit ou se développe automatiquement lorsque la page défile

- **Masquer automatiquement** : masquer automatiquement lors du défilement de la page.
- **Développement automatique** : Affiché automatiquement lorsque la page défile vers le haut ou que la barre de défilement disparaît.
- **Optimisation des performances** : utilisez `requestAnimationFrame` pour limiter le défilement.
- **Ressources de libération** : les écouteurs et les observateurs d'événements sont automatiquement supprimés lorsque le composant est détruit.

## Utilisez la démo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Lien</a>
      </c-nav>
    </nav>
    <div class="content">Faites glisser votre doigt vers le bas pour masquer la navigation, faites glisser votre doigt vers le haut pour l'afficher</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Mode d'emploi

Doit être utilisé comme élément enfant ou descendant de `<c-vs>`.

## statut de style

Le composant bascule le nom de classe suivant sur `nav.Nav` :

- `D` : afficher l'état (glisser vers le bas ou vers le haut).
- `I` : État de transition masqué.
- `H` : état masqué.

## variable de style

- `--top` : automatiquement mis à jour par le composant avec une valeur de hauteur négative, utilisée pour le positionnement et la transition.