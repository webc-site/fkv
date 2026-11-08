# შექმენით და აჩვენეთ ამომხტარი ფენა დახურვის ღილაკით

- ფუნქციური ზარი ამომხტარი ფენის შესაქმნელად და ჩვენებისთვის
- ჩაშენებული დახურვის ღილაკი, დააწკაპუნეთ დახურვისთვის და ამოიღეთ ელემენტი DOM-დან
- დააჭირეთ Esc-ს შეყვანის ველის დეფოკუსირებისთვის ან ამომხტარი ფენის დახურვისთვის

## გამოიყენეთ დემო ვერსია

```html
<button>დააწკაპუნეთ გასახსნელად</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "ტექსტი";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## ინტერფეისის პარამეტრები

ნაგულისხმევი ექსპორტის ფუნქცია აბრუნებს `HTMLDialogElement`, ხოლო `Box` და `X` სტილის კლასები დაემატა დიალოგის ელემენტს.

## სტილის კლასი

### `.Box.X`

გამოიყენება `dialog` ელემენტზე, განსაზღვრავს დიალოგის სტილს.

### `a.X`

გამოიყენება `a` ელემენტზე, განსაზღვრავს დახურვის ღილაკის სტილს.