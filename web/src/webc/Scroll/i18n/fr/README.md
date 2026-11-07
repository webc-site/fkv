# barre de défilement virtuelle

- **Défilement bidirectionnel** : permet un défilement vertical (`c-vs`) et horizontal (`c-hs`).
- **Réponse de taille** : surveillez les modifications du contenu et de la taille de la zone, mettez automatiquement à jour la taille et la position de la barre de défilement et masquez automatiquement lorsqu'il n'y a pas de débordement.
- **Support interactif** : prend en charge le défilement par molette, le déplacement du curseur et le positionnement par clic.
- **Masquer le statut** : affichez et élargissez le curseur lors du survol ou du défilement, et masquez-le automatiquement à l'arrêt.

## défilement vertical

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

## défilement horizontal

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

## partie de style

Personnalisez le style des éléments internes du Shadow DOM via `::part()` :

- `::part(scroll)` : faites défiler la zone de contenu.
- `::part(bar)` : piste de barre de défilement.
- `::part(si)` : curseur de la barre de défilement.
- `::part(i)` : couche d'habillage interne.
- `::part(drag)` : la piste de la barre de défilement à l'état de déplacement.

## cours de style

- `.drag` : appliqué à l'élément `body`, indiquant qu'il est dans un état de défilement par glissement.

## Propriétés personnalisées

- `--si-bg` : couleur d'arrière-plan du curseur.
- `--si-anim` : animation du curseur.
- `--cursorScrollhSvg` : le curseur de la souris lorsque la barre de défilement horizontale est survolée.
- `--cursorScrollvSvg` : le curseur de la souris lorsque la barre de défilement verticale est survolée.