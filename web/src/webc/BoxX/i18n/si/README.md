# වසන්න බොත්තමක් සමඟ උත්පතන ස්ථරයක් සාදා සංදර්ශන කරන්න

- උත්පතන ස්තරය සෑදීමට සහ ප්‍රදර්ශනය කිරීමට ක්‍රියාකාරී ඇමතුම
- බිල්ට්-ඉන් වසන බොත්තම, වසා දැමීමට ක්ලික් කර DOM වෙතින් මූලද්‍රව්‍යය ඉවත් කරන්න
- ආදාන පෙට්ටිය නාභිගත කිරීමට හෝ උත්පතන ස්තරය වැසීමට Esc ඔබන්න

## Demo භාවිතා කරන්න

```html
<button>උත්පතන කිරීමට ක්ලික් කරන්න</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "පෙළ";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## අතුරුමුහුණත් පරාමිතීන්

පෙරනිමි නිර්යාත ශ්‍රිතය `HTMLDialogElement` ආපසු ලබා දෙන අතර `Box` සහ `X` විලාස පන්ති සංවාද මූලද්‍රව්‍යයට එක් කර ඇත.

## ශෛලිය පන්තිය

### `.Box.X`

`dialog` මූලද්‍රව්‍යයට යොදන ලද, සංවාද විලාසය සඳහන් කරයි.

### `a.X`

`a` මූලද්‍රව්‍යයට යොදන ලද, වසන බොත්තම් විලාසය සඳහන් කරයි.