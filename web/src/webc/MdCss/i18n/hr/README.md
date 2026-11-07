# Stilovi tipografije optimizirani za iskustvo čitanja

- **Osnovni slog**: Prilagodba naslovima, odlomcima, podebljanom, kurzivu, precrtanom tekstu, umetnutom kodu, slikama i razdjelnim crtama.
- **Animacija poveznice**: Prikažite podvučenu animaciju kada lebdite.
- **Popis projekata**: Neuređeni popisi koriste grafičke oznake, uređeni popisi koriste zaokružene pozadinske serijske brojeve i podržavaju gniježđenje na više razina.
- **Popis zadataka**: Pruža stil potvrdnog okvira, podržava status obaveza i dovršeno.
- **Savjeti upozorenja**: Podržava pet vrsta okvira upozorenja: Napomena, Savjet, Važno, Upozorenje i Pažnja.
- **Tablice i kod**: Omogućuje označavanje retka tablice lebdećim pokazivačem i blokove koda sa sjenama i gradijentnim pozadinama.

## Koristite demo

```html
<div class="Md">
  <h1>titula</h1>
  <p>Tekst odlomka i <a href="#">veza</a></p>
  <hr>
  <h2>Naslov druge razine</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Stavka zadatka</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Sadržaj bilješke</p>
  </blockquote>
</div>
```

## stilska klasa

Stilovi se primjenjuju na HTML elemente u `.Md` spremniku.

### Stil okvira upozorenja

Dodajte sljedeći naziv klase elementu `blockquote` s nazivom klase `.q` za uključivanje/isključivanje okvira upozorenja:

- `note`: Bilješke
- `tip`: Savjet
- `important`: Važno
- `warning`: upozorenje
- `caution`: Napomena