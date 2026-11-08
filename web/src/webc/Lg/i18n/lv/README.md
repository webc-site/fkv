# Matēta stikla fons un malu izcelšanas efekts

Daļēji caurspīdīga apmale un iekšējā ēna rada mikroreljefa malu izcēlumus.
Kā pogu un ievades lodziņa komponentu pamata stila atkarība.

## Izmantojiet demonstrāciju

```html
<main class="Lg">
  <article>
    <h3>virsraksts</h3>
    <pre>Satura teksts</pre>
  </article>
</main>
```

## stila klase

### `.Lg`
Kartes pamatstils. Lietojiet `backdrop-filter` izplūšanu, `border` mikroreljefu apmali un `box-shadow` iekšējo ēnu. Lietojiet filtru `svg/glass.svg#h`, virzot kursoru.

### `main.Lg`
Lapas galvenā bloka konteiners. Noapaļoti stūri `24px` ar adaptīvu polsterējumu (`16px`, ja platums ir mazāks par `800px`, `24px` pretējā gadījumā).

### `.Btn`, `.BtnC`, `.Input`
Pamatkomponentu klases nosaukums. Pēc šī stila ieviešanas jūs automātiski iegūsit elastīgu izkārtojumu, hierarhiskas attiecības un matēta stikla pamatīpašības.