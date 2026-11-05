# Rendre et afficher des démonstrations de composants, de la documentation et des instructions d'installation

- Extraire et analyser le titre, l'introduction et le contenu du document README
- Sandbox pour exécuter des démos interactives de composants et isoler les styles
- Convertissez les blocs de code des documents en présentations interactives
- Disposition adaptative, affichage en colonnes sur écran large, affichage de commutation d'onglets sur écran étroit

## Utilisez la démo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Initialiser et obtenir l'interface d'opération
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Paramètres d'interface

### `WebCDemo(el)`

Initialisez le composant.

- **paramètre**
  - `el` :`HTMLElement`, conteneur de composants.
- **Valeur renvoyée** :`[setMod, setMd, unMount]`
  - `setMod(mod)` : configurez le module de démonstration.
  - `setMd(name, md, lang)` : définissez le nom du composant, le contenu du document README et la langue actuelle.
  - `unMount()` : déchargez le composant et nettoyez l'écouteur d'événements.

### Attribut `mod` du module de démonstration

- `CSS` :`string`, le chemin de la feuille de style du composant.
- `HTM` :`string`, le modèle HTML du composant.
- `default` :`function(root)`, fonction d'initialisation du composant, reçoit le conteneur racine fantôme.

## cours de style

### `.M`

zone de contenu.

### `.L`

Le panneau de gauche contient des instructions et de la documentation.

### `.R`

Panneau de droite, contenant la démo.

### `c-nav`

Barre de navigation à écran étroit.