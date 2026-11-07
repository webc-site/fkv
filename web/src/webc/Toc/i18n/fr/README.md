# Composant de navigation dans l'annuaire

- **Tiroir réactif** : affiche la barre latérale sur un écran large, se réduit automatiquement sur un écran étroit et coopère avec le tiroir `c-side` et le bouton de déclenchement du titre.
- **Pilote multilingue** : prend en charge le dictionnaire d'objets ou les fonctions multilingues, répond automatiquement au changement de langue
- **Sous-répertoire automatique** : analysez automatiquement les titres `h1`~`h6` dans le conteneur cible pour générer un répertoire secondaire et surveiller la surbrillance du défilement de la fenêtre de hachage.
- **Chargement asynchrone** : cliquez sur un élément du répertoire pour obtenir automatiquement le texte Markdown de manière asynchrone et l'injecter dans la zone de contenu, affichant `Ing` l'animation de chargement

## Utilisez la démo

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "utiliser le composant"
});
</script>
```

## Paramètres d'interface

### Propriétés et méthodes

- `toc.prefix` : obtenez ou définissez la chaîne de préfixe de routage (telle que `"doc"`, la barre oblique finale est facultative), la clé `README` sera automatiquement mappée sur un chemin vide (c'est-à-dire le préfixe lui-même)
- `toc.li` : obtenir ou définir la fonction de génération d'annuaire multilingue `(lang) => ({ key: title })`
- `toc.mdUrl` : obtenez ou définissez la fonction de génération d'URL Markdown `(key, lang) => url`
- `toc.target` : obtenez ou définissez le nœud de rendu cible associé (tel que `c-md`). S'il n'est pas spécifié, il trouvera automatiquement `c-md` / `main` / `article`
- `toc.load(key)` : Déclenchez manuellement le chargement du document et l'extension du répertoire correspondant à la clé
- `toc.sync()` : réanalysez manuellement le titre dans le nœud cible actuel, synchronisez le répertoire secondaire et mettez l'état en surbrillance