# Φόρτωση και εμφάνιση λίστας καρτών ασύγχρονα

- **Ασύγχρονη φόρτωση**: Λήψη δεδομένων μέσω μιας ασύγχρονης συνάρτησης
- **Εμφάνιση κατάστασης**: Υποστηρίζει τρεις καταστάσεις: φόρτωση, άδεια δεδομένα και κάρτα απόδοσης.
- **Ευέλικτη διάταξη**: Οι κάρτες είναι ταξινομημένες σε σειρές

## Χρησιμοποιήστε το demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Τίτλος " + i, "Περιγραφή " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Παράμετροι διεπαφής

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, ασύγχρονη λειτουργία φόρτωσης δεδομένων. Η μορφή κάθε δευτερεύοντος κλειδιού είναι `[όνομα, εισαγωγή, σύνδεσμος]`.

## τάξη στυλ

- `.CardLi`: εξωτερικό στοιχείο
- `.Card`: στοιχείο κάρτας
- `.Ing`: Φόρτωση κινούμενων στοιχείων