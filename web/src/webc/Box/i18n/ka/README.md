# შექმენით და აჩვენეთ ამომხტარი ფენა

- ფუნქციური ზარი ამომხტარი ფენის შესაქმნელად და ჩვენებისთვის
- ავტომატურად ამოიღეთ ელემენტები DOM-დან დახურვისას
- თავიდან აიცილეთ Esc გასაღების გაუქმების ქცევა

## გამოიყენეთ დემო ვერსია

```html
<button>დააწკაპუნეთ გასახსნელად</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "სათაური";
  description.textContent = "ტექსტი";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## ინტერფეისის პარამეტრები

### ნაგულისხმევი ექსპორტირებული ფუნქციები

`Box()`

- **დაბრუნების მნიშვნელობა**: `HTMLDialogElement`, `Box` სტილის კლასის დიალოგის ელემენტი დამატებულია.

## სტილის კლასი

### `.Box`

ვრცელდება `dialog` ელემენტზე პოზიციონირების, ცენტრირებისა და ფონის ნიღბის დაბინდვის დასაყენებლად.

### `.Lg`

სტილის კლასი მიმართა შიდა შინაარსს, დააყენა ფონის ფერი, მომრგვალებული კუთხეები, ჩრდილი და ვერტიკალური განლაგება და განსაზღვრა შიდა `h3` და `p` განლაგება.