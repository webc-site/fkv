# Charger et afficher la liste des cartes de manière asynchrone

- **Chargement asynchrone** : obtenez des données via une fonction asynchrone
- **Affichage de l'état** : prend en charge trois états : chargement, données vides et carte de rendu.
- **Mise en page flexible** : les cartes sont disposées en rangées

## Utilisez la démo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titre" + i, "Description " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Paramètres d'interface

### CardLi(loader)

- **loader** : `() => Promise<Array<[string, string, string]>>`, fonction de chargement de données asynchrone. Le format de chaque sous-clé est `[name, introduction, link]`.

## cours de style

- `.CardLi` : élément externe
- `.Card` : élément de carte
- `.Ing` : Chargement de l'élément animé