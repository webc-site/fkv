# Créer et afficher une couche contextuelle

- Appel fonctionnel pour créer et afficher une couche contextuelle
- Supprimer automatiquement les éléments du DOM à la fermeture
- Empêcher le comportement d'annulation de la touche Échap

## Utilisez la démo

```html
<button>Cliquez pour apparaître</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Titre" ;
  description.textContent = "Texte";
  button.className = "Btn Main";
  bouton.textContent = "OK" ;
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Paramètres d'interface

### Fonctions exportées par défaut

`Box()`

- **Valeur renvoyée** : `HTMLDialogElement`, l'élément de dialogue de la classe de style `Box` a été ajouté.

## cours de style

### `.Box`

S'applique à l'élément `dialog` pour définir le positionnement, le centrage et le flou du masque d'arrière-plan.

### `.Lg`

Une classe de style appliquée au contenu interne, définissant la couleur d'arrière-plan, les coins arrondis, l'ombre et la disposition verticale, et définissant la disposition des `h3` et `p` internes.