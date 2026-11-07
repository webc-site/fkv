# ಕ್ಲೋಸ್ ಬಟನ್‌ನೊಂದಿಗೆ ಪಾಪ್ಅಪ್ ಲೇಯರ್ ಅನ್ನು ರಚಿಸಿ ಮತ್ತು ಪ್ರದರ್ಶಿಸಿ

- ಪಾಪ್ಅಪ್ ಲೇಯರ್ ರಚಿಸಲು ಮತ್ತು ಪ್ರದರ್ಶಿಸಲು ಕ್ರಿಯಾತ್ಮಕ ಕರೆ
- ಅಂತರ್ನಿರ್ಮಿತ ಕ್ಲೋಸ್ ಬಟನ್, DOM ನಿಂದ ಅಂಶವನ್ನು ಮುಚ್ಚಲು ಮತ್ತು ತೆಗೆದುಹಾಕಲು ಕ್ಲಿಕ್ ಮಾಡಿ
- ಇನ್‌ಪುಟ್ ಬಾಕ್ಸ್ ಅನ್ನು ಡಿಫೋಕಸ್ ಮಾಡಲು ಅಥವಾ ಪಾಪ್‌ಅಪ್ ಲೇಯರ್ ಅನ್ನು ಮುಚ್ಚಲು Esc ಒತ್ತಿರಿ

## ಡೆಮೊ ಬಳಸಿ

```html
<button>ಪಾಪ್ ಅಪ್ ಮಾಡಲು ಕ್ಲಿಕ್ ಮಾಡಿ</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "ಪಠ್ಯ";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## ಇಂಟರ್ಫೇಸ್ ನಿಯತಾಂಕಗಳು

ಡೀಫಾಲ್ಟ್ ರಫ್ತು ಕಾರ್ಯವು `HTMLDialogElement` ಅನ್ನು ಹಿಂದಿರುಗಿಸುತ್ತದೆ ಮತ್ತು `Box` ಮತ್ತು `X` ಶೈಲಿಯ ತರಗತಿಗಳನ್ನು ಸಂವಾದ ಅಂಶಕ್ಕೆ ಸೇರಿಸಲಾಗಿದೆ.

## ಶೈಲಿ ವರ್ಗ

### `.Box.X`

`dialog` ಅಂಶಕ್ಕೆ ಅನ್ವಯಿಸಲಾಗಿದೆ, ಸಂವಾದ ಶೈಲಿಯನ್ನು ನಿರ್ದಿಷ್ಟಪಡಿಸುತ್ತದೆ.

### `a.X`

`a` ಅಂಶಕ್ಕೆ ಅನ್ವಯಿಸಲಾಗಿದೆ, ಮುಚ್ಚು ಬಟನ್ ಶೈಲಿಯನ್ನು ನಿರ್ದಿಷ್ಟಪಡಿಸುತ್ತದೆ.