# Créer et afficher un calque contextuel avec un bouton de fermeture

- Appel fonctionnel pour créer et afficher une couche contextuelle
- Bouton de fermeture intégré, cliquez pour fermer et supprimer l'élément du DOM
- Appuyez sur Échap pour défocaliser la zone de saisie ou fermer le calque contextuel

## Utilisez la démo

```html
<button>Cliquez pour apparaître</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "texte" ;
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Paramètres d'interface

La fonction d'exportation par défaut renvoie `HTMLDialogElement` et les classes de style `Box` et `X` ont été ajoutées à l'élément de dialogue.

## cours de style

### `.Box.X`

Appliqué à l'élément `dialog`, spécifie le style de dialogue.

### `a.X`

Appliqué à l'élément `a`, spécifie le style du bouton de fermeture.