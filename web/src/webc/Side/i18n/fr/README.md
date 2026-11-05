# Panneaux de tiroirs coulissants sur le côté

- Créez automatiquement des structures de défilement internes
- Cliquez sur la zone extérieure pour réduire automatiquement
- Faites glisser votre doigt vers la gauche et la droite pour fermer
- Positionnement coulissant latéral dans les directions gauche et droite
- Proposer des événements d'ouverture et de clôture

## Utilisez la démo

```html
<button class="open-btn">Ouvrir</button>

<c-side class="right">
  <div>Contenu du tiroir</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interface

### méthode

- `open(el)` : Fonction d'exportation, ouverture du tiroir
- `close(el)` : Fonction d'exportation, fermeture du tiroir
- `el.open()` : méthode d'instance, tiroir ouvert
- `el.close()` : méthode d'instance, fermez le tiroir

### événement

- `open` : se déclenche lorsqu'il est allumé
- `close` : se déclenche lorsqu'il est fermé

## cours de style

- `.right` : positionnement coulissant vers la droite
- `.top` : positionnement coulissant
- `.bottom` : glisser hors du positionnement
- (S'il n'est pas défini, la valeur par défaut est de glisser vers la gauche)
- `.open` : statut ouvert
- `.H` : état masqué
- `.T` : état de transition de l'animation