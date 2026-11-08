# Composant d'article de blog

- **Regroupement automatique des années** : classez automatiquement les années en fonction de la date de l'article et organisez-les dans l'ordre inverse par année
- **Format de date simplifié** : supprimez automatiquement le préfixe de l'année lors de l'affichage et affichez uniquement le mois et le jour.
- **Résumé et texte Markdown** : aperçu du résumé de la liste intégré et rendu du texte complet d'un seul article
- **Titre tridimensionnel dégradé** : intégrez `H1Txt` pour afficher le titre et la date de sortie

## Utilisez la démo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publier": ["titre", "résumé", "2026-07-27"]
};

// Basculer pour charger le texte de l'article spécifié (passer une chaîne vide pour revenir à la liste)
blog.load("2026/publish");
</script>
```

## Paramètres d'interface

### Propriétés et méthodes

- `blog.prefix` : obtenez ou définissez le préfixe du lien de l'article (tel que `"/blog/"`)
- `blog.mdUrl` : obtenez ou définissez la fonction de génération d'adresse d'article Markdown `(key, lang) => url`
- `blog._` : obtenir ou définir le dictionnaire de métadonnées de la liste d'articles `{ [key]: [title, summary, date] }`
- `blog.load(key)` : Basculez pour charger le contenu de l'article de la clé spécifiée, passez une chaîne vide `""` pour revenir à la liste des blogs