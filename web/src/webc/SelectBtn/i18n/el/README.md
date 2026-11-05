# Συνδυασμός αναπτυσσόμενου πλαισίου επιλογής και κουμπιού

- Συνδέστε το πλαίσιο επιλογής και το κουμπί οριζόντια
- Αποθηκεύστε αυτόματα την επιλεγμένη κατάσταση στο localStorage
- Εντόπιση κλικ κουμπιών και αποστολή προσαρμοσμένων συμβάντων

## Χρησιμοποιήστε το demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Επιλογή 1</option>
    <option value="option2">Επιλογή 2</option>
  </c-select>
  <button>τρέξιμο</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ιδιοκτησία

- `key`: όνομα κλειδιού που χρησιμοποιείται για τη διατήρηση της τοπικής αποθήκευσης
- `value`: επιλεγμένη τιμή

## συμβάν

- `submit`: Αποστέλλεται όταν κάνετε κλικ στο κουμπί, το `e.value` φέρει την τρέχουσα επιλεγμένη τιμή