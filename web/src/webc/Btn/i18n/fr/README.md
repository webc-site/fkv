# Bouton ordinaire et bouton icône ronde

## Nom de la classe

- Bouton normal`.Btn`
- bouton icône `.BtnC`
- État de surbrillance principal`.Btn.Main`

## utiliser

Ajoutez un nom de classe à l'élément :

```html
<!-- Bouton normal -->
<button class="Btn">confirmer</button>

<!-- point culminant principal -->
<button class="Btn Main">soumettre</button>

<!-- bouton de lien -->
<a class="Btn" href="https://github.com">bouton de lien</a>
```

## style d'icône

L'icône de `BtnC` peut être personnalisée via `background-image` :

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```