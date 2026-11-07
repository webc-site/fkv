# Συνηθισμένο κουμπί και κουμπί στρογγυλού εικονιδίου

## Όνομα τάξης

- Κανονικό κουμπί`.Btn`
- κουμπί εικονιδίου `.BtnC`
- Κατάσταση κύριας επισήμανσης`.Btn.Main`

## χρήση

Προσθέστε ένα όνομα κλάσης στο στοιχείο:

```html
<!-- Κανονικό κουμπί -->
<button class="Btn">επιβεβαιώνω</button>

<!-- κύριο σημείο αναφοράς -->
<button class="Btn Main">υποτάσσομαι</button>

<!-- κουμπί σύνδεσης -->
<a class="Btn" href="https://github.com">κουμπί σύνδεσης</a>
```

## στυλ εικονιδίου

Το εικονίδιο για το `BtnC` μπορεί να προσαρμοστεί μέσω του `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```