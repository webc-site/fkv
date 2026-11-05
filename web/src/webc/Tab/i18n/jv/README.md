# Tab sing bisa diganti

- Status label sing dipilih digabungake kanthi lancar karo area isi ing ngisor iki.
- Spesifikasi struktural: Elemen anak tetep nggunakake `nav > a` minangka pandhu arah label, lan unsur anak langsung `b` digunakake minangka area isi.
- Gathukake tag `a[value]` kanthi otomatis karo simpul isi `b[slot]`.
- Ndhukung kanthi otomatis tetep kaca aktif saiki kanggo `localStorage` liwat atribut `key`.
- Ndhukung nemtokake kaca tab sing diaktifake liwat atribut `value` (yen ora ana cache lan `value`, tag karo `class="A"` utawa tag pisanan bakal dipilih kanthi gawan).
- Implementasi DOM Ringan sing entheng, ora ana alangan Shadow DOM, tingkat kebebasan sing dhuwur ing kustomisasi gaya.

## Gunakake demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">apel</a>
    <a value="banana">gedhang</a>
    <a value="orange">jeruk keprok</a>
  </nav>
  <b slot="apple">Apple minangka woh sing nutrisi</b>
  <b slot="banana">Pisang minangka woh tropis</b>
  <b slot="orange">Jeruk sugih ing vitamin C</b>
</c-tab>
```

## properti

- `key`: jeneng kunci sing digunakake kanggo kegigihan localStorage
- `value`: nilai sing dipilih saiki

## acara adat

- `change`: Micu nalika tab diowahi, `e.value` minangka nilai `value` saka tag sing saiki diaktifake.