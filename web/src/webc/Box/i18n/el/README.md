# Δημιουργία και εμφάνιση αναδυόμενου στρώματος

- Λειτουργική κλήση για δημιουργία και εμφάνιση αναδυόμενου στρώματος
- Αυτόματη αφαίρεση στοιχείων από το DOM όταν είναι κλειστό
- Αποτρέψτε τη συμπεριφορά ακύρωσης κλειδιού Esc

## Χρησιμοποιήστε το demo

```html
<button>Κάντε κλικ για να εμφανιστεί</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Τίτλος";
  description.textContent = "Κείμενο";
  button.className = "Btn Main";
  button.textContent = "ΟΚ";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Παράμετροι διεπαφής

### Προεπιλεγμένες εξαγόμενες λειτουργίες

`Box()`

- **Επιστρεφόμενη τιμή**: `HTMLDialogElement`, προστέθηκε το στοιχείο διαλόγου της κλάσης στυλ `Box`.

## τάξη στυλ

### `.Box`

Εφαρμόζεται στο στοιχείο `dialog` για να ορίσετε τη θέση, το κεντράρισμα και το θάμπωμα μάσκας φόντου.

### `.Lg`

Μια κλάση στυλ που εφαρμόζεται στο εσωτερικό περιεχόμενο, ορίζοντας το χρώμα φόντου, τις στρογγυλεμένες γωνίες, τη σκιά και την κατακόρυφη διάταξη και ορίζοντας τη διάταξη των εσωτερικών `h3` και `p` .